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
//! [`ShutdownHandle`]: zim_runtime::ShutdownHandle

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;

use zim_core::blobs::BlobsProvider;
use zim_core::iroh::{self, Endpoint, NodeAddr, Router, BLOBS_ALPN};
use zim_crypto::{PrivateKey, PublicKey};
use zim_runtime::Service;

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
    /// Take-once slots, consumed by [`Peer::shutdown`].
    ///
    /// Both values are *consumed* at shutdown, not just dropped:
    /// `Router::shutdown(self)` takes ownership, and the runner's
    /// `JoinHandle` must be awaited exactly once. `Option::take()`
    /// expresses "move it out of a shared struct, exactly once" —
    /// the second `shutdown()` call finds `None` and no-ops, which
    /// is what makes shutdown idempotent.
    ///
    /// The `Mutex` is for the clones: `Peer` is `Clone` via the
    /// outer `Arc`, and clones live in different tasks (axum
    /// handlers, the effect runner, the spawned shutdown-watcher),
    /// so concurrent `shutdown()` calls can race — e.g. a ctrl-C
    /// signal against service teardown. The lock makes the `take()`
    /// atomic: exactly one caller gets the value. (`Router` and
    /// `JoinHandle` themselves are never cloned — only moved out.)
    ///
    /// Could be `std::sync::Mutex` — the guard is never held across
    /// an `.await` (`take()` returns owned, guard drops before the
    /// shutdown future runs) — but the tokio mutex costs nothing on
    /// a once-per-process path.
    router: Mutex<Option<Router>>,
    runner: Mutex<Option<JoinHandle<()>>>,
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
            let Some(target) = share.reach().copied() else {
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
        // Tell the effect runner to exit. The receiver is held by the
        // runner; sending here ensures `changed()` fires.
        let _ = self.0.shutdown_tx.send(());
        if let Some(handle) = self.0.runner.lock().await.take() {
            let _ = handle.await;
        }
    }

    /// Spawn a task that watches `shutdown_rx` and tears the peer
    /// down when it fires. Returns a [`JoinHandle`] ready to register
    /// with [`ShutdownHandle::push`](zim_runtime::ShutdownHandle::push).
    ///
    /// Takes `&self` and clones the cheap Arc internally — the
    /// caller doesn't need to type `peer.clone()`. Equivalent in
    /// effect to `<Peer<L, P> as Service>::spawn(self.clone(), rx)`.
    pub fn spawn(&self, shutdown_rx: watch::Receiver<()>) -> JoinHandle<()> {
        let peer = self.clone();
        tokio::spawn(async move {
            let mut rx = shutdown_rx;
            let _ = rx.changed().await;
            peer.shutdown().await;
        })
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
        let _ = shutdown_rx.changed().await;
        peer.shutdown().await;
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
        let runner = tokio::spawn(run_effects(coord.clone(), effect_rx, shutdown_rx));

        // Register the iroh-blobs + sync protocols on the router.
        let blobs_handler = (*blobs.protocol().clone()).clone();
        let router = Router::builder(endpoint.clone())
            .accept(BLOBS_ALPN, blobs_handler)
            .accept(ALPN, SyncProtocol::new(coord.clone()))
            .spawn();

        Ok(Peer(Arc::new(PeerInner {
            secret,
            endpoint,
            coord,
            router: Mutex::new(Some(router)),
            runner: Mutex::new(Some(runner)),
            shutdown_tx,
        })))
    }
}
