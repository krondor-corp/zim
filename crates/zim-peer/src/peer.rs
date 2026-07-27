//! The `Peer` — a fully-wired sync daemon for one identity.
//!
//! Bundles together:
//! - an iroh [`Endpoint`] (UDP bind + QUIC),
//! - a [`SyncCoordinator`] (vault registry + effect queue),
//! - an iroh [`Router`] registered for the blobs + sync ALPNs,
//! - the background effect runner task.
//!
//! Built via [`Peer::builder()`]. Plug into a [`ShutdownHandle`] via
//! the inherent [`Peer::spawn`] method:
//!
//! ```ignore
//! let peer = Peer::builder()
//!     .with_secret(secret)
//!     .with_log(MemoryVaultLog::new())
//!     .with_blobs(blobs)
//!     .build()
//!     .await?;
//!
//! let (mut shutdown, shutdown_rx) = ShutdownHandle::new();
//! shutdown.push("peer", peer.spawn(shutdown_rx));
//! // ... use `peer` to register vaults, submit effects, etc. ...
//! shutdown.wait().await;
//! ```
//!
//! [`ShutdownHandle`]: crate::runtime::ShutdownHandle

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;

use crate::blobs::BlobsProvider;
use crate::iroh::{self, Endpoint, NodeAddr, Router, BLOBS_ALPN};
use crate::runtime::Service;
use zim_crypto::{PrivateKey, PublicKey};

use crate::accept::{AcceptAll, AcceptPolicy};
use crate::coordinator::{run_effects, SyncCoordinator};
use crate::effect::Effect;
use crate::iroh_transport::{IrohPeerSender, SyncProtocol, ALPN};
use crate::Vault;
use zim_core::vault::{Head, VaultId, VaultLog};

/// Failure modes for [`Peer::vault`].
///
/// Two categories, matching the [`FsError`](zim_core::fs::FsError)
/// shape:
///
/// - **Domain condition** callers can react to: [`Self::NotFound`] —
///   the id isn't tracked in the log.
/// - **Backing-layer failure** — every error from beneath the lookup
///   (log I/O, manifest decode, blob fetch, share recovery) folds
///   into [`Self::Backing`]. The original error is preserved inside
///   the [`anyhow::Error`] and walkable via
///   [`std::error::Error::source`].
#[derive(Debug, thiserror::Error)]
pub enum VaultLookupError {
    #[error("vault {0} not found")]
    NotFound(VaultId),
    #[error(transparent)]
    Backing(#[from] anyhow::Error),
}

/// One row of [`Peer::list_vaults`]. `name` is `None` when the
/// manifest can't be opened; `error` carries the reason so callers
/// can surface "vault present in log but unreadable" instead of
/// silently dropping it.
#[derive(Debug, Clone)]
pub struct VaultListing {
    pub id: VaultId,
    pub name: Option<String>,
    pub error: Option<String>,
}

/// Cheap-clone handle to a running peer. All clones share the same
/// underlying state (Arc-internals); the `Endpoint`, `Router`, and
/// effect runner live until the last clone drops *or* `shutdown` is
/// invoked explicitly via the [`Service`] machinery.
#[derive(Clone)]
pub struct Peer<L: VaultLog>(Arc<PeerInner<L>>);

struct PeerInner<L: VaultLog> {
    secret: PrivateKey,
    endpoint: Endpoint,
    coord: Arc<SyncCoordinator<L>>,
    /// Take-once slots (`Option::take()` = "move out exactly once",
    /// making teardown idempotent; the `Mutex` covers racing clones —
    /// e.g. ctrl-C against service teardown).
    ///
    /// **Known blind spot — the iroh router.** `Router::spawn()` runs
    /// its accept loop on a task iroh owns internally; all we hold is
    /// this handle, whose only affordance is `shutdown()`. If that
    /// loop dies, no `JoinHandle` surfaces it — the liveness watcher
    /// below cannot see it, and the peer would look healthy while
    /// refusing connections. Accepted: the accept loop is iroh's code
    /// (our panic surface is `tasks`), and its death shows up
    /// indirectly — inbound sync stops and the reconcile sweep's
    /// outbound pulls keep working, which is a visible asymmetry in
    /// the logs. Revisit if iroh ever exposes the task handle.
    router: Mutex<Option<Router>>,
    /// The peer's own tasks (effect runner, reconcile sweep). Taken by
    /// the [`Service`] watcher, which supervises liveness; if no
    /// watcher was spawned (tests calling [`Peer::shutdown`]
    /// directly), `shutdown` drains it instead.
    tasks: Mutex<Option<tokio::task::JoinSet<&'static str>>>,
    shutdown_tx: watch::Sender<()>,
}

impl<L> Peer<L>
where
    L: VaultLog + Clone + 'static,
    L::Error: std::error::Error + Send + Sync + 'static,
{
    pub fn builder() -> PeerBuilder<L> {
        PeerBuilder::default()
    }

    pub fn id(&self) -> PublicKey {
        self.0.secret.public()
    }

    pub fn secret(&self) -> &PrivateKey {
        &self.0.secret
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.0.endpoint
    }

    pub fn coord(&self) -> &Arc<SyncCoordinator<L>> {
        &self.0.coord
    }

    /// Notify every shareholder of `vault` (other than ourselves) that
    /// we just advanced to `head`. The sole sync push: each recipient
    /// turns it into a `PullFromPeer` — bootstrapping the vault if it's
    /// new (and we're in their address book) or fast-forwarding if not.
    /// Fire-and-forget; failures are logged, never bubbled. Write
    /// handlers should call this after `vault.save()`.
    pub async fn announce_head(&self, vault: &Vault<L>, head: Head) {
        let self_pk = self.0.secret.public();
        let manifest = vault.manifest();

        // One message per shareholder: dial the `via` host for a hosted
        // client (pre-resolved at share time, so no network lookup here),
        // else the client itself — and carry `recipient = client` so a
        // relay knows whose push this is. We do *not* dedupe by dial
        // target: two clients behind the same hub are two recipients, so
        // the hub must hear about each. Drop our own share.
        for (client, share) in manifest.shares().iter() {
            // `reach()` = where we dial for this share (the `via` host, else
            // the client); `client` = who it's for (the recipient).
            let Some(target) = share.reach() else {
                continue;
            };
            if target == self_pk {
                continue;
            }
            if let Err(e) = self
                .0
                .coord
                .submit(Effect::AnnounceHead {
                    peer_id: target,
                    vault_id: vault.id(),
                    head: Box::new(head.clone()),
                    recipient: *client,
                })
                .await
            {
                tracing::warn!(
                    peer = %target.to_hex(),
                    vault_id = %vault.id(),
                    "failed to enqueue AnnounceHead: {e}"
                );
            }
        }
    }

    /// Resolve `vault_id` to an open [`Vault`]. The canonical accessor
    /// for any caller that has a vault id and needs the vault behind
    /// it. Returns:
    ///
    /// - [`VaultLookupError::NotFound`] when the id isn't present in
    ///   the log (recoverable: HTTP layers map to 404),
    /// - [`VaultLookupError::Backing`] for anything underneath (log
    ///   I/O, manifest decode, blob missing, share recovery, …).
    pub async fn vault(&self, vault_id: VaultId) -> Result<Vault<L>, VaultLookupError> {
        let log = self.0.coord.log();
        let exists = log.exists(vault_id).await.map_err(anyhow::Error::from)?;
        if !exists {
            return Err(VaultLookupError::NotFound(vault_id));
        }
        Ok(self.0.coord.open_vault(vault_id).await?)
    }

    /// Every vault known to this peer, with its display name and an
    /// optional per-row error when the manifest can't be opened
    /// (share bootstrap left the log populated but the head blob is
    /// missing locally, etc.). Per-vault failures ride along inside
    /// the [`VaultListing`] so one broken vault doesn't tank the
    /// whole list; only the initial log enumeration produces an
    /// outer `Err`.
    pub async fn list_vaults(&self) -> anyhow::Result<Vec<VaultListing>> {
        let log = self.0.coord.log();
        let ids = log.list_vaults().await.map_err(anyhow::Error::from)?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            match self.0.coord.open_vault(id).await {
                Ok(vault) => {
                    let name = vault.manifest().name().to_string();
                    out.push(VaultListing {
                        id,
                        name: Some(name),
                        error: None,
                    });
                }
                Err(e) => {
                    // Expected for a relay/hub: it mirrors ciphertext but holds
                    // no share, so it can't open vaults it stores. The failure
                    // is returned in `error` for callers that surface it; at
                    // default level it's just noise (one line per vault, per
                    // poll), so log it at debug.
                    let msg = e.to_string();
                    tracing::debug!(vault_id = %id, "list_vaults: open failed: {msg}");
                    out.push(VaultListing {
                        id,
                        name: None,
                        error: Some(msg),
                    });
                }
            }
        }
        Ok(out)
    }

    /// Current [`NodeAddr`] for this peer. Use with [`Peer::introduce`]
    /// on the other side when peers should know about each other
    /// without going through pkarr/relay discovery.
    pub fn node_addr(&self) -> NodeAddr {
        self.0.endpoint.node_addr()
    }

    /// Inject another peer's `NodeAddr` so we can dial them without
    /// DHT/relay discovery. Tests + local-dev path.
    pub fn introduce(&self, addr: NodeAddr) -> anyhow::Result<()> {
        self.0
            .endpoint
            .add_node_addr_with_source(addr, "peer_introduce")?;
        Ok(())
    }

    /// Stop the router (close the endpoint, refuse new accepts), then
    /// signal the effect runner to exit and await it. Idempotent —
    /// calling multiple times is safe; second call is a no-op.
    pub async fn shutdown(&self) {
        if let Some(router) = self.0.router.lock().await.take() {
            if let Err(e) = router.shutdown().await {
                tracing::warn!("router shutdown error: {e}");
            }
        }
        // Tell the internal tasks to exit, then drain them — unless
        // the Service watcher already took the set (it drains after
        // this returns).
        let _ = self.0.shutdown_tx.send(());
        if let Some(mut tasks) = self.0.tasks.lock().await.take() {
            while tasks.join_next().await.is_some() {}
        }
    }

    /// Spawn the peer's supervisor: watches `shutdown_rx` for teardown
    /// AND the internal tasks for early death (see the [`Service`]
    /// impl). Returns a [`JoinHandle`] ready to register with
    /// [`ShutdownHandle::push`](crate::runtime::ShutdownHandle::push).
    ///
    /// Takes `&self` and clones the cheap Arc internally — the
    /// caller doesn't need to type `peer.clone()`. This shadows
    /// [`Service::spawn`] and delegates to it.
    pub fn spawn(&self, shutdown_rx: watch::Receiver<()>) -> JoinHandle<()> {
        let peer = self.clone();
        tokio::spawn(<Self as Service>::run(peer, shutdown_rx))
    }
}

/// [`BlobsProtocol`](crate::iroh::BlobsProtocol) wrapped in the accept
/// policy's [`accept_blob`](AcceptPolicy::accept_blob) gate. Consulted
/// once per inbound blob connection, before any request is served.
#[derive(Clone)]
struct GatedBlobs {
    inner: crate::iroh::BlobsProtocol,
    accept: Arc<dyn AcceptPolicy>,
}

impl std::fmt::Debug for GatedBlobs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatedBlobs").finish_non_exhaustive()
    }
}

impl iroh::ProtocolHandler for GatedBlobs {
    fn accept(
        &self,
        conn: iroh::Connection,
    ) -> impl std::future::Future<Output = Result<(), iroh::AcceptError>> + Send {
        let inner = self.inner.clone();
        let accept = self.accept.clone();
        async move {
            let remote = conn.remote_node_id().map_err(iroh::AcceptError::from)?;
            let sender = iroh::from_iroh_public_key(&remote);
            if !accept.accept_blob(&sender).await {
                tracing::info!(
                    sender = %sender.to_hex(),
                    "blob connection dropped (sender not accepted)"
                );
                conn.close(0u32.into(), b"not authorized");
                return Ok(());
            }
            inner.accept(conn).await
        }
    }
}

/// `Peer` as a [`Service`] for callers that want trait-based uniformity
/// with other long-running components.
///
/// Most callers should prefer the inherent [`Peer::spawn`] method —
/// `peer.spawn(rx)` — which avoids typing the clone at the call site.
#[async_trait]
impl<L> Service for Peer<L>
where
    L: VaultLog + Clone + 'static,
    L::Error: std::error::Error + Send + Sync + 'static,
{
    type State = Peer<L>;

    async fn run(peer: Self, mut shutdown_rx: watch::Receiver<()>) {
        // Take ownership of the peer's internal tasks: this watcher is
        // the liveness supervisor, not just a shutdown proxy. Any task
        // finishing BEFORE the signal means the sync engine is
        // degraded — return early so `ShutdownHandle::wait`'s
        // exited-before-shutdown tripwire fires (exit 2 → the service
        // manager restarts the process). The iroh router's accept loop
        // is NOT covered — see the blind-spot note on `PeerInner`.
        let taken = peer.0.tasks.lock().await.take();
        let Some(mut tasks) = taken else {
            // Already torn down elsewhere; just wait out the signal.
            let _ = shutdown_rx.changed().await;
            return;
        };
        tokio::select! {
            _ = shutdown_rx.changed() => {
                peer.shutdown().await;
                while tasks.join_next().await.is_some() {}
            }
            Some(res) = tasks.join_next() => {
                match res {
                    Ok(name) => {
                        tracing::error!(task = name, "peer task exited before shutdown")
                    }
                    Err(e) if e.is_panic() => tracing::error!("peer task panicked: {e}"),
                    Err(e) => tracing::error!("peer task join error: {e}"),
                }
                peer.shutdown().await;
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
            }
        }
    }
}

// ─── Builder ──────────────────────────────────────────────────────

/// Discovery policy. Off by default (peers exchange `NodeAddr`s via
/// [`Peer::introduce`]); flip to pkarr DHT for production deployments.
#[derive(Debug, Clone, Default)]
pub enum Discovery {
    /// No discovery wired. Peers learn about each other only via
    /// explicit [`Peer::introduce`]. Tests + hermetic local dev.
    #[default]
    Off,
    /// pkarr DHT discovery. Requires network egress.
    Pkarr,
}

pub struct PeerBuilder<L: VaultLog> {
    secret: Option<PrivateKey>,
    log: Option<L>,
    accept: Option<Arc<dyn AcceptPolicy>>,
    /// Period between reconcile sweeps (catch-up pulls for pushes
    /// missed while offline). `None` disables; default 5 minutes.
    sync_interval: Option<std::time::Duration>,
    blobs: Option<BlobsProvider>,
    discovery: Discovery,
    effect_queue_size: Option<usize>,
    info: Option<crate::coordinator::DaemonInfo>,
}

impl<L: VaultLog> Default for PeerBuilder<L> {
    fn default() -> Self {
        Self {
            secret: None,
            log: None,
            accept: None,
            blobs: None,
            discovery: Discovery::Off,
            effect_queue_size: None,
            info: None,
            sync_interval: Some(std::time::Duration::from_secs(300)),
        }
    }
}

impl<L> PeerBuilder<L>
where
    L: VaultLog + Clone + 'static,
    L::Error: std::error::Error + Send + Sync + 'static,
{
    pub fn with_secret(mut self, secret: PrivateKey) -> Self {
        self.secret = Some(secret);
        self
    }

    pub fn with_log(mut self, log: L) -> Self {
        self.log = Some(log);
        self
    }

    /// Set the inbound [`AcceptPolicy`]. Defaults to
    /// [`AcceptAll`](crate::AcceptAll) — fine for a single peer or a
    /// test. The daemon supplies a contacts-backed policy, the hub a
    /// `user_peers`-backed one.
    /// Override the reconcile sweep period (`None` disables it).
    pub fn with_sync_interval(mut self, interval: Option<std::time::Duration>) -> Self {
        self.sync_interval = interval;
        self
    }

    pub fn with_accept_policy(mut self, accept: Arc<dyn AcceptPolicy>) -> Self {
        self.accept = Some(accept);
        self
    }

    pub fn with_blobs(mut self, blobs: BlobsProvider) -> Self {
        self.blobs = Some(blobs);
        self
    }

    /// Use the pkarr DHT for peer discovery. Default is no discovery
    /// (explicit `Peer::introduce` only).
    pub fn with_pkarr_discovery(mut self) -> Self {
        self.discovery = Discovery::Pkarr;
        self
    }

    pub fn with_effect_queue_size(mut self, size: usize) -> Self {
        self.effect_queue_size = Some(size);
        self
    }

    /// Set the identity/version metadata the coordinator advertises
    /// in `Pong` replies. Defaults to "unknown" at process start.
    pub fn with_info(mut self, info: crate::coordinator::DaemonInfo) -> Self {
        self.info = Some(info);
        self
    }

    /// Bind the endpoint, build the coordinator, register both
    /// ALPNs, spawn the effect runner. Returns a running [`Peer`].
    pub async fn build(self) -> anyhow::Result<Peer<L>> {
        let secret = self.secret.unwrap_or_else(PrivateKey::generate);
        let log = self
            .log
            .ok_or_else(|| anyhow::anyhow!("PeerBuilder: missing log"))?;
        // Inbound acceptance: defaults to accept-all (single peer / tests).
        let accept = self
            .accept
            .unwrap_or_else(|| Arc::new(AcceptAll) as Arc<dyn AcceptPolicy>);
        let blobs = self
            .blobs
            .ok_or_else(|| anyhow::anyhow!("PeerBuilder: missing blobs"))?;
        let effect_queue_size = self.effect_queue_size.unwrap_or(64);

        // Bind endpoint with the configured discovery policy.
        let iroh_secret = iroh::to_iroh_secret_key(&secret);
        let mut endpoint_builder = Endpoint::builder().secret_key(iroh_secret);
        match self.discovery {
            Discovery::Off => {}
            Discovery::Pkarr => {
                endpoint_builder = endpoint_builder.add_discovery(iroh::DhtDiscovery::builder());
            }
        }
        let endpoint = endpoint_builder.bind().await?;

        // Coordinator + sender + effect channel.
        let sender = Arc::new(IrohPeerSender::new(endpoint.clone()));
        let info = self.info.unwrap_or_default();
        let (coord, effect_rx) = SyncCoordinator::new_with_info(
            blobs.clone(),
            log,
            accept,
            endpoint.clone(),
            secret.clone(),
            sender,
            effect_queue_size,
            info,
        );

        // Spawn the effect runner with its own shutdown channel.
        let (shutdown_tx, shutdown_rx) = watch::channel(());

        // Every internal task the peer owns goes into one JoinSet —
        // each returns its name so the Service watcher can say *which*
        // task died. A task finishing before the shutdown signal is a
        // liveness failure the supervisor must see (see the Service
        // impl below).
        let mut tasks = tokio::task::JoinSet::new();
        {
            let coord = coord.clone();
            let rx = shutdown_rx.clone();
            tasks.spawn(async move {
                run_effects(coord, effect_rx, rx).await;
                "effect-runner"
            });
        }

        // Periodic reconcile: one sweep shortly after boot (missed-
        // while-offline catch-up, after discovery has a moment to
        // settle), then every `sync_interval`.
        if let Some(interval) = self.sync_interval {
            let coord = coord.clone();
            let mut shutdown = shutdown_rx.clone();
            tasks.spawn(async move {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {}
                    _ = shutdown.changed() => return "reconcile",
                }
                loop {
                    coord.reconcile_pass().await;
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {}
                        _ = shutdown.changed() => return "reconcile",
                    }
                }
            });
        }

        // Register the iroh-blobs + sync protocols on the router. The
        // blobs handler is gated by the accept policy: without the gate,
        // anyone who can dial the endpoint fetches any blob by hash.
        let blobs_handler = GatedBlobs {
            inner: (*blobs.protocol().clone()).clone(),
            accept: coord.accept_policy(),
        };
        let router = Router::builder(endpoint.clone())
            .accept(BLOBS_ALPN, blobs_handler)
            .accept(ALPN, SyncProtocol::new(coord.clone()))
            .spawn();

        Ok(Peer(Arc::new(PeerInner {
            secret,
            endpoint,
            coord,
            router: Mutex::new(Some(router)),
            tasks: Mutex::new(Some(tasks)),
            shutdown_tx,
        })))
    }
}
