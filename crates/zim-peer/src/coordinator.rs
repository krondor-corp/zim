//! `SyncCoordinator` — vault registry + background effect runner.
//!
//! Responsibilities:
//!
//! 1. **Vault opener.** [`open_vault`](Self::open_vault) re-reads the
//!    head from the [`VaultLog`] on every call. Previously this layer
//!    cached `Vault<L>` by id, but `Vault` is *not* Arc-backed:
//!    `Clone` deep-copies the in-memory manifest, so concurrent
//!    handlers each mutated isolated copies of the cached state and
//!    every `save()` started from the same stale snapshot — the log
//!    grew sideways at height 1, never advancing. Re-opening per call
//!    costs one head-blob read; correctness wins.
//! 2. **Log-only peer queries.** Answers [`HeadRequest`] /
//!    [`ProbeRequest`] / [`AncestorRequest`] directly from the
//!    [`VaultLog`] without opening the vault — most peer traffic
//!    never wakes a working copy. These methods return *immediately*
//!    so the wire layer can ship a reply.
//! 3. **Background effect runner.** Anything that's not "answer this
//!    request" is an [`Effect`]. Effects are pushed into an mpsc
//!    queue; a background task drains them, [`tokio::spawn`]-ing each
//!    one so a slow effect can't block faster ones.
//!
//! The split keeps reply latency tight: any `handle_*_request` call
//! is just one log query plus a struct allocation.

use std::sync::Arc;

use tokio::sync::mpsc;
use zim_crypto::{PrivateKey, PublicKey};

use zim_core::blobs::{BlobStore, BlobsProvider};
use zim_core::fs::{Manifest, MergeResult};
use zim_core::iroh::{Downloader, Endpoint, Hash, Shuffled};
use zim_core::linked_data::Link;
use zim_core::vault::{Head, VaultId, VaultLog};

use crate::chain;
use crate::effect::Effect;
use crate::messages::{
    Ack, AncestorReply, AncestorRequest, HeadAdvanced, HeadReply, HeadRequest, PingRequest,
    PongReply, ProbeReply, ProbeRequest, ShareOffered,
};
use crate::peers::PeerStore;
use crate::wire_protocol::PeerSender;
use crate::Vault;

/// Identity and runtime info the coordinator reports in `Pong`
/// replies — set once at construction.
#[derive(Debug, Clone)]
pub struct DaemonInfo {
    /// Display version (typically `BuildInfo::version`). Whatever the
    /// caller picks shows up in `zim peers ping`.
    pub version: String,
    /// Process start time. `uptime_secs` is `now - started_at`.
    pub started_at: std::time::Instant,
}

impl Default for DaemonInfo {
    fn default() -> Self {
        Self {
            version: "unknown".into(),
            started_at: std::time::Instant::now(),
        }
    }
}

/// The coordinator. Generic over `L: VaultLog` and `P: PeerStore`.
pub struct SyncCoordinator<L: VaultLog + 'static, P: PeerStore + 'static> {
    blobs: BlobsProvider,
    log: L,
    peers: P,
    endpoint: Endpoint,
    secret: PrivateKey,
    peer_sender: Arc<dyn PeerSender>,
    /// Outbound queue for background effects. Cloned on every
    /// `submit` so the channel survives even if the runner exits.
    effect_tx: mpsc::Sender<Effect>,
    info: DaemonInfo,
}

impl<L, P> SyncCoordinator<L, P>
where
    L: VaultLog + Clone + 'static,
    L::Error: std::error::Error + Send + Sync + 'static,
    P: PeerStore + 'static,
{
    /// Build a coordinator. Returns `(coordinator, effect_rx)`. The
    /// caller should spawn [`run_effects`] with the receiver to start
    /// the background runner.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        blobs: BlobsProvider,
        log: L,
        peers: P,
        endpoint: Endpoint,
        secret: PrivateKey,
        peer_sender: Arc<dyn PeerSender>,
        effect_queue_size: usize,
    ) -> (Arc<Self>, mpsc::Receiver<Effect>) {
        Self::new_with_info(
            blobs,
            log,
            peers,
            endpoint,
            secret,
            peer_sender,
            effect_queue_size,
            DaemonInfo::default(),
        )
    }

    /// As [`Self::new`] but with caller-supplied identity/version
    /// metadata reported in `Pong` replies. The daemon builds via
    /// this; tests use the no-arg variant.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_info(
        blobs: BlobsProvider,
        log: L,
        peers: P,
        endpoint: Endpoint,
        secret: PrivateKey,
        peer_sender: Arc<dyn PeerSender>,
        effect_queue_size: usize,
        info: DaemonInfo,
    ) -> (Arc<Self>, mpsc::Receiver<Effect>) {
        let (effect_tx, effect_rx) = mpsc::channel(effect_queue_size);
        let coord = Arc::new(Self {
            blobs,
            log,
            peers,
            endpoint,
            secret,
            peer_sender,
            effect_tx,
            info,
        });
        (coord, effect_rx)
    }

    pub fn log(&self) -> &L {
        &self.log
    }

    pub fn peers(&self) -> &P {
        &self.peers
    }

    pub fn blobs(&self) -> &BlobsProvider {
        &self.blobs
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// The transport-shaped peer messenger. Exposed so callers can
    /// fire one-off round-trip messages (e.g. `Ping`) without going
    /// through the effect queue.
    pub fn peer_sender(&self) -> &Arc<dyn PeerSender> {
        &self.peer_sender
    }

    // ── Vault opener ──

    /// Open the [`Vault`] for `vault_id` at its current log head.
    ///
    /// Re-reads the head every call. The previous design cached the
    /// opened `Vault<L>` and handed clones to handlers, but
    /// `Vault::clone` deep-copies the in-memory manifest — two
    /// concurrent handlers ended up mutating isolated copies of the
    /// same stale state. Each `save()` would then bump height from
    /// the *cached* genesis, and the log grew sideways with multiple
    /// entries at height 1 instead of advancing. Re-opening per call
    /// costs one manifest blob read + decrypt; correctness wins.
    pub async fn open_vault(&self, vault_id: VaultId) -> anyhow::Result<Vault<L>> {
        Vault::open(vault_id, self.blobs.clone(), self.log.clone(), &self.secret)
            .await
            .map_err(|e| anyhow::anyhow!("Vault::open({vault_id}): {e}"))
    }

    // ── Fast-path peer queries (log-only, never open a vault) ──
    //
    // Every `handle_*` method below has the same shape:
    //     `(sender: PublicKey, req: RequestStruct) -> ReplyStruct`
    // because the `wire_protocol!` macro's generated dispatch calls
    // them uniformly. Handlers that don't care about `sender` or
    // `req` accept and discard them.

    pub async fn handle_head(&self, _sender: PublicKey, req: HeadRequest) -> HeadReply {
        let head = if self.log.exists(req.vault_id).await.unwrap_or(false) {
            self.log.head(req.vault_id, None).await.ok()
        } else {
            None
        };
        HeadReply {
            vault_id: req.vault_id,
            head,
        }
    }

    pub async fn handle_probe(&self, _sender: PublicKey, req: ProbeRequest) -> ProbeReply {
        let highest = self
            .log
            .probe(req.vault_id, &req.sample)
            .await
            .ok()
            .flatten();
        ProbeReply {
            vault_id: req.vault_id,
            highest,
        }
    }

    /// One-way push handler: enqueue a follow-up pull and return
    /// `Ack`. The actual sync happens on the background runner;
    /// failures to enqueue are logged but not surfaced to the sender
    /// (their copy of the head is no longer load-bearing).
    pub async fn handle_head_advanced(&self, sender: PublicKey, req: HeadAdvanced) -> Ack {
        if let Err(e) = self
            .submit(Effect::PullFromPeer {
                vault_id: req.vault_id,
                peer_id: sender,
            })
            .await
        {
            tracing::warn!("failed to enqueue PullFromPeer: {e}");
        }
        Ack
    }

    /// Bootstrap a vault we've never seen, from a [`ShareOffered`]
    /// push another peer just sent us. Walks the manifest chain back
    /// to genesis (populating the log), then registers the vault in
    /// the coordinator so it appears in `list_vaults`.
    ///
    /// **Synchronous v1.** Read availability waits for the chain walk
    /// to complete — alice unreachable mid-walk leaves the vault
    /// missing on bob's side until she comes back. Tracked under
    /// `docs/research/optimistic-share-acceptance.md`.
    ///
    /// Errors are logged here; the wire layer always sees an `Ack`
    /// because the sender doesn't gate anything on success. (Alice
    /// has already forgotten the offer by the time her Ack flushes.)
    pub async fn handle_share_offered(&self, sender: PublicKey, req: ShareOffered) -> Ack {
        if let Err(e) = self.share_offered_inner(sender, req).await {
            tracing::warn!("share_offered bootstrap failed: {e}");
        }
        Ack
    }

    async fn share_offered_inner(
        &self,
        sender: PublicKey,
        req: ShareOffered,
    ) -> anyhow::Result<()> {
        let ShareOffered { vault_id, head } = req;

        // If we already know this vault, treat the offer as a hint
        // about a possibly-newer head: queue a normal pull from the
        // sender and return. No re-bootstrap needed. We accept hints
        // from any peer — they're cheap and the sender must have
        // already passed the chain-validity check at our last sync.
        if self.log.exists(vault_id).await.unwrap_or(false) {
            return self
                .submit(Effect::PullFromPeer {
                    vault_id,
                    peer_id: sender,
                })
                .await;
        }

        // Fresh vault: spam gate. A cold bootstrap walks the chain
        // back to genesis and persists every manifest blob — too
        // expensive to do on demand for anyone on the wire. Drop the
        // offer unless we've explicitly added the sender to our peer
        // book. The sender still gets an Ack from the wire layer; we
        // just silently refuse to bootstrap.
        match self.peers.knows(&sender).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::info!(
                    sender = %sender.to_hex(),
                    vault_id = %vault_id,
                    "ShareOffered from unknown peer dropped (not in peer book)"
                );
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    sender = %sender.to_hex(),
                    "peer book lookup failed, dropping ShareOffered: {e}"
                );
                return Ok(());
            }
        }

        // Fresh vault from a known peer: walk the chain back to
        // genesis, fetching each manifest blob from the sender and
        // appending it into the log. Once that returns, the log is
        // populated and the vault is openable through the normal
        // `open_vault` path. No working copy needed for the walk —
        // manifests are signed plaintext.
        self.apply_chain(vault_id, head, None, vec![sender])
            .await
            .map_err(|e| anyhow::anyhow!("apply_chain bootstrap for {vault_id}: {e}"))?;
        Ok(())
    }

    /// Reply to `Ping`. Renders identity from our own keypair and
    /// uptime from the start instant; no log / vault access.
    pub async fn handle_ping(&self, _sender: PublicKey, _req: PingRequest) -> PongReply {
        PongReply {
            peer_id: self.secret.public().to_hex(),
            version: self.info.version.clone(),
            uptime_secs: self.info.started_at.elapsed().as_secs(),
        }
    }

    pub async fn handle_ancestor(&self, _sender: PublicKey, req: AncestorRequest) -> AncestorReply {
        if !self.log.exists(req.vault_id).await.unwrap_or(false) {
            return AncestorReply::NotFound {
                vault_id: req.vault_id,
            };
        }

        let heights = self
            .log
            .has(req.vault_id, req.initiator_head.link.clone())
            .await
            .unwrap_or_default();
        if !heights.is_empty() {
            return AncestorReply::Found {
                vault_id: req.vault_id,
                ancestor: req.initiator_head,
            };
        }

        AncestorReply::NeedProbe {
            vault_id: req.vault_id,
        }
    }

    // ── Effect submission (callers from anywhere) ──

    /// Submit an effect for background execution. Returns immediately;
    /// the work runs on the background runner (see [`run_effects`]).
    ///
    /// Returns `Err` if the runner has shut down (channel closed) or
    /// the queue is full and we couldn't enqueue.
    pub async fn submit(&self, effect: Effect) -> anyhow::Result<()> {
        self.effect_tx
            .send(effect)
            .await
            .map_err(|_| anyhow::anyhow!("effect queue closed"))
    }

    /// Try-submit variant for callers that can't `await` (e.g. inside
    /// non-async closures). Returns `false` if the queue is full or
    /// closed; effect is dropped.
    pub fn try_submit(&self, effect: Effect) -> bool {
        self.effect_tx.try_send(effect).is_ok()
    }

    /// Execute a single effect inline. The background runner uses
    /// this from inside a `tokio::spawn`. Exposed for tests that want
    /// to bypass the queue.
    pub async fn execute(&self, effect: Effect) -> anyhow::Result<()> {
        match effect {
            Effect::PullFromPeer { vault_id, peer_id } => {
                self.pull_from_peer(vault_id, peer_id).await
            }
            Effect::AnnounceHead {
                peer_id,
                vault_id,
                head,
            } => {
                let _: Ack = self
                    .peer_sender
                    .send_head_advanced(peer_id, HeadAdvanced { vault_id, head })
                    .await?;
                Ok(())
            }
            Effect::OfferShare {
                peer_id,
                vault_id,
                head,
            } => {
                let _: Ack = self
                    .peer_sender
                    .send_share_offered(peer_id, ShareOffered { vault_id, head })
                    .await?;
                Ok(())
            }
        }
    }

    /// End-to-end pull from a single peer. Three round-trips:
    /// 1. `HeadRequest` — learn remote head + height.
    /// 2. `ProbeRequest` — find ancestor via our exponential sample.
    /// 3. Local `apply_chain + merge_with` — download + apply.
    async fn pull_from_peer(&self, vault_id: VaultId, peer_id: PublicKey) -> anyhow::Result<()> {
        let reply = self
            .peer_sender
            .send_head(peer_id, HeadRequest { vault_id })
            .await?;
        let Some(target) = reply.head else {
            tracing::info!(
                vault_id = %vault_id,
                peer_id = %peer_id.to_hex(),
                "peer doesn't have this vault"
            );
            return Ok(());
        };

        // No-op if we're already at or past their height.
        if self.log.exists(vault_id).await? {
            let ours = self.log.head(vault_id, None).await?;
            if ours.height >= target.height {
                tracing::debug!(%vault_id, our_height = ours.height, target_height = target.height, "already up to date");
                return Ok(());
            }
        }

        let sample = self.log.exponential_sample(vault_id).await?;
        let probe = self
            .peer_sender
            .send_probe(peer_id, ProbeRequest { vault_id, sample })
            .await?;
        let ancestor = probe.highest.map(|h| h.link);

        // Open fresh from the log's current head. If the vault is
        // already in the log (the shareholder happy path), `Vault::open`
        // does the work; on `ShareNotFound` we're a relay (hub) and
        // delegate to the log-only `relay_pull`. If the vault is
        // *not* in the log yet (first-time bootstrap from a
        // `ShareOffered` push), download the head manifest blob and
        // open against it directly.
        let open_result = if self.log.exists(vault_id).await? {
            Vault::open(vault_id, self.blobs.clone(), self.log.clone(), &self.secret).await
        } else {
            self.blobs
                .download_hash(target.link.hash(), vec![peer_id], &self.endpoint)
                .await
                .map_err(|e| anyhow::anyhow!("download head manifest blob: {e}"))?;
            Vault::open_with_head(
                vault_id,
                target.link.clone(),
                self.blobs.clone(),
                self.log.clone(),
                &self.secret,
            )
            .await
        };
        let mut vault = match open_result {
            Ok(v) => v,
            Err(zim_core::vault::VaultError::Fs(zim_core::fs::FsError::ShareNotFound)) => {
                // Relay path: walk the chain by raw blob fetches and
                // append to the log. No Vault, no decrypt.
                return crate::relay_pull::apply_chain_log_only(
                    vault_id,
                    target,
                    ancestor,
                    peer_id,
                    &self.blobs,
                    &self.log,
                    &self.endpoint,
                )
                .await;
            }
            Err(e) => return Err(anyhow::anyhow!("open vault for pull: {e}")),
        };

        let target_link = target.link.clone();
        self.apply_chain(vault_id, target, ancestor.clone(), vec![peer_id])
            .await?;
        self.merge_vault(&mut vault, &target_link, ancestor.as_ref())
            .await?;
        Ok(())
    }

    // ── Sync orchestration (formerly the `Vault<L>` adapter) ──
    //
    // These only need what the coordinator already owns — blobs, log,
    // endpoint, identity key. No working copy is involved except in
    // `merge_vault`, which mutates a caller-opened [`Vault`].

    /// Sync `vault_id` up to a known remote head: find a common
    /// ancestor by walking the remote chain against our log, then
    /// download + append the missing manifests. No merge — the
    /// working copy is untouched.
    pub async fn sync_vault(
        &self,
        vault_id: VaultId,
        target: Head,
        peer_ids: Vec<PublicKey>,
    ) -> anyhow::Result<()> {
        let exists = self.log.exists(vault_id).await?;
        if exists {
            let ours = self.log.head(vault_id, None).await?;
            if ours.height >= target.height {
                return Ok(());
            }
        }

        let common_ancestor = if exists {
            self.find_common_ancestor(vault_id, &target.link, &peer_ids)
                .await?
                .map(|h| h.link)
        } else {
            None
        };

        // With genesis-derived ids, two verified chains for the same
        // vault ALWAYS share an ancestor (at worst genesis itself). A
        // missing ancestor means the remote chain doesn't belong to
        // this vault — reject it rather than modeling "divergence".
        if exists && common_ancestor.is_none() {
            anyhow::bail!(
                "vault {vault_id}: remote chain shares no ancestor with ours — \
                 chain rejected (not this vault)"
            );
        }

        self.apply_chain(vault_id, target, common_ancestor, peer_ids)
            .await
    }

    /// Download the manifest chain from `target_link` back to
    /// `ancestor` (or genesis), verify authorship, and append every
    /// manifest into the log + download its pinned content blobs.
    pub async fn apply_chain(
        &self,
        vault_id: VaultId,
        target: Head,
        ancestor: Option<Link>,
        peer_ids: Vec<PublicKey>,
    ) -> anyhow::Result<()> {
        if self.log.exists(vault_id).await? {
            let ours = self.log.head(vault_id, None).await?;
            if ours.height >= target.height {
                return Ok(());
            }
        }

        let stop_link = ancestor.as_ref();
        let trusted_base: Option<Manifest> = match stop_link {
            Some(link) => Some(self.blobs.get_cbor(link).await?),
            None => None,
        };

        let manifests = self
            .download_manifest_chain(&target.link, stop_link, &peer_ids, trusted_base.as_ref())
            .await?;

        if manifests.is_empty() {
            return Ok(());
        }

        // Self-certification check. When the walk bottomed out at a
        // genesis (no ancestor bound, or the chain ended before
        // reaching one), the genesis blob's hash IS the vault id —
        // anything else is a different vault wearing this id on the
        // wire, and gets rejected before a single log append.
        let (first_manifest, first_link) = manifests.first().unwrap();
        if *first_manifest.previous() == Link::default() {
            let derived = VaultId::from_genesis_link(first_link);
            if derived != vault_id {
                anyhow::bail!(
                    "chain genesis hashes to {derived}, not the claimed vault id {vault_id} — \
                     chain rejected"
                );
            }
        }

        let latest = &manifests.last().unwrap().0;
        latest
            .verify_author(trusted_base.as_ref())
            .map_err(|e| anyhow::anyhow!("verify author: {e}"))?;

        for (manifest, link) in &manifests {
            self.append_manifest(vault_id, manifest, link).await?;
            self.download_pins(manifest).await?;
        }

        Ok(())
    }

    /// Merge a synced remote head into an open working copy and save
    /// the result as a new version. `ancestor` is the common base the
    /// pull already discovered (None = full replay from genesis).
    pub async fn merge_vault(
        &self,
        vault: &mut Vault<L>,
        incoming_link: &Link,
        ancestor: Option<&Link>,
    ) -> anyhow::Result<(MergeResult, Link)> {
        let merge_result = chain::merge(
            vault.fs(),
            vault.manifest_link(),
            vault.private_key(),
            incoming_link,
            ancestor,
        )
        .await?;
        let link = vault.save().await.map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok((merge_result, link))
    }

    // ── Internal sync helpers ──

    async fn append_manifest(
        &self,
        vault_id: VaultId,
        manifest: &Manifest,
        link: &Link,
    ) -> anyhow::Result<()> {
        let previous = {
            let p = manifest.previous().clone();
            if p == Link::default() {
                None
            } else {
                Some(p)
            }
        };
        self.log
            .append(
                vault_id,
                manifest.name().to_string(),
                link.clone(),
                previous,
                manifest.height(),
            )
            .await?;
        Ok(())
    }

    async fn download_hash(&self, hash: Hash, peer_ids: &[PublicKey]) -> anyhow::Result<()> {
        if self.blobs.stat(&hash).await.unwrap_or(false) {
            return Ok(());
        }
        let downloader = Downloader::new(self.blobs.protocol().store(), &self.endpoint);
        let discovery = Shuffled::new(
            peer_ids
                .iter()
                .map(zim_core::iroh::to_iroh_public_key)
                .collect(),
        );
        downloader.download(hash, discovery).await?;
        Ok(())
    }

    async fn find_common_ancestor(
        &self,
        vault_id: VaultId,
        start_link: &Link,
        peer_ids: &[PublicKey],
    ) -> anyhow::Result<Option<Head>> {
        let mut current_link = start_link.clone();
        loop {
            self.download_hash(current_link.hash(), peer_ids).await?;
            let manifest: Manifest = self.blobs.get_cbor(&current_link).await?;
            let heights = self.log.has(vault_id, current_link.clone()).await?;
            if !heights.is_empty() {
                return Ok(Some(Head::new(current_link, manifest.height())));
            }
            if *manifest.previous() == Link::default() {
                return Ok(None);
            }
            current_link = manifest.previous().clone();
        }
    }

    async fn download_manifest_chain(
        &self,
        start_link: &Link,
        stop_link: Option<&Link>,
        peer_ids: &[PublicKey],
        trusted_base: Option<&Manifest>,
    ) -> anyhow::Result<Vec<(Manifest, Link)>> {
        let mut manifests = Vec::new();
        let mut current_link = start_link.clone();

        loop {
            self.download_hash(current_link.hash(), peer_ids).await?;
            let manifest: Manifest = self.blobs.get_cbor(&current_link).await?;

            if stop_link.is_some_and(|sl| sl == &current_link) {
                break;
            }

            manifests.push((manifest.clone(), current_link.clone()));

            if *manifest.previous() == Link::default() {
                break;
            }
            current_link = manifest.previous().clone();
        }

        manifests.reverse();

        let mut previous: Option<&Manifest> = trusted_base;
        for (manifest, _) in &manifests {
            manifest
                .verify_author(previous)
                .map_err(|e| anyhow::anyhow!("verify author: {e}"))?;
            previous = Some(manifest);
        }

        Ok(manifests)
    }

    async fn download_pins(&self, manifest: &Manifest) -> anyhow::Result<()> {
        let peer_ids: Vec<PublicKey> = manifest
            .shares()
            .iter()
            .filter_map(|(_, share)| share.identity().pubkey().copied())
            .collect();

        if peer_ids.is_empty() {
            return Ok(());
        }

        for hash in manifest.pins().iter() {
            self.download_hash(*hash, &peer_ids).await?;
        }

        Ok(())
    }
}

/// Background effect runner.
///
/// Drains the channel, [`tokio::spawn`]-ing each effect on its own
/// task so independent effects run in parallel and a slow one doesn't
/// block faster ones following it.
///
/// **Future:** swap this for an [Apalis](https://crates.io/crates/apalis)-backed
/// runner when we need persistence, retry, or visibility. Same
/// interface (consume effects from a channel); the implementation
/// changes behind the trait. See the comment in `Cargo.toml` for the
/// scope of that swap.
pub async fn run_effects<L, P>(
    coord: Arc<SyncCoordinator<L, P>>,
    mut rx: mpsc::Receiver<Effect>,
    mut shutdown_rx: tokio::sync::watch::Receiver<()>,
) where
    L: VaultLog + Clone + 'static,
    L::Error: std::error::Error + Send + Sync + 'static,
    P: PeerStore + 'static,
{
    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                tracing::debug!("run_effects: shutdown signal");
                break;
            }
            maybe_effect = rx.recv() => match maybe_effect {
                Some(effect) => {
                    let coord = coord.clone();
                    tokio::spawn(async move {
                        if let Err(e) = coord.execute(effect).await {
                            tracing::error!("effect failed: {e}");
                        }
                    });
                }
                None => {
                    tracing::debug!("run_effects: effect channel closed");
                    break;
                }
            }
        }
    }
}

// ── Test helpers ──

/// In-memory [`PeerSender`] that records all outbound messages for
/// inspection.
#[derive(Debug, Default, Clone)]
pub struct MemoryPeerSender {
    pub sent: Arc<tokio::sync::RwLock<Vec<SentMessage>>>,
}

#[derive(Debug, Clone)]
pub enum SentMessage {
    Head(PublicKey, HeadRequest),
    Probe(PublicKey, ProbeRequest),
    Ancestor(PublicKey, AncestorRequest),
    HeadAdvanced(PublicKey, HeadAdvanced),
    ShareOffered(PublicKey, ShareOffered),
    Ping(PublicKey, PingRequest),
}

#[async_trait::async_trait]
impl PeerSender for MemoryPeerSender {
    async fn send_head(&self, peer_id: PublicKey, req: HeadRequest) -> anyhow::Result<HeadReply> {
        let vault_id = req.vault_id;
        self.sent
            .write()
            .await
            .push(SentMessage::Head(peer_id, req));
        Ok(HeadReply {
            vault_id,
            head: None,
        })
    }
    async fn send_probe(
        &self,
        peer_id: PublicKey,
        req: ProbeRequest,
    ) -> anyhow::Result<ProbeReply> {
        let vault_id = req.vault_id;
        self.sent
            .write()
            .await
            .push(SentMessage::Probe(peer_id, req));
        Ok(ProbeReply {
            vault_id,
            highest: None,
        })
    }
    async fn send_ancestor(
        &self,
        peer_id: PublicKey,
        req: AncestorRequest,
    ) -> anyhow::Result<AncestorReply> {
        let vault_id = req.vault_id;
        self.sent
            .write()
            .await
            .push(SentMessage::Ancestor(peer_id, req));
        Ok(AncestorReply::NotFound { vault_id })
    }
    async fn send_head_advanced(
        &self,
        peer_id: PublicKey,
        req: HeadAdvanced,
    ) -> anyhow::Result<Ack> {
        self.sent
            .write()
            .await
            .push(SentMessage::HeadAdvanced(peer_id, req));
        Ok(Ack)
    }
    async fn send_share_offered(
        &self,
        peer_id: PublicKey,
        req: ShareOffered,
    ) -> anyhow::Result<Ack> {
        self.sent
            .write()
            .await
            .push(SentMessage::ShareOffered(peer_id, req));
        Ok(Ack)
    }
    async fn send_ping(&self, peer_id: PublicKey, req: PingRequest) -> anyhow::Result<PongReply> {
        self.sent
            .write()
            .await
            .push(SentMessage::Ping(peer_id, req));
        Ok(PongReply {
            peer_id: peer_id.to_hex(),
            version: "memory-sender".into(),
            uptime_secs: 0,
        })
    }
}
