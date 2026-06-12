use std::sync::Arc;

use futures::future::BoxFuture;
use tokio::sync::watch::Receiver as WatchReceiver;
use zim_core::iroh::BlobsProtocol;
use zim_core::iroh::Connection;
use zim_core::iroh::{AcceptError, ProtocolHandler, Router};

use zim_crypto::PublicKey;

mod peer_builder;
mod peer_inner;
mod protocol;
pub mod sync;

pub use protocol::{PingReplyStatus, ALPN};
pub use sync::{SyncJob, SyncProvider, SyncTarget};
pub use zim_core::blobs::BlobsStore;

pub use zim_core::iroh::NodeAddr;

pub use peer_builder::PeerBuilder;
pub use peer_inner::Peer;

/// Wire-level peer classification for sync routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerType {
    Member,
    Relay,
    Anonymous,
}

impl std::fmt::Display for PeerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerType::Member => write!(f, "Member"),
            PeerType::Relay => write!(f, "Relay"),
            PeerType::Anonymous => write!(f, "Anonymous"),
        }
    }
}

/// Classify a peer against a manifest.
pub fn classify_peer(manifest: &zim_core::fs::Manifest, public_key: &PublicKey) -> PeerType {
    if manifest.has_share(public_key) {
        PeerType::Member
    } else if manifest.is_relay(public_key) {
        PeerType::Relay
    } else {
        PeerType::Anonymous
    }
}

/// Spawn the peer with protocol router
///
/// This starts the iroh protocol router for handling incoming connections.
/// The peer's sync provider is responsible for managing its own background workers.
///
/// # Arguments
///
/// * `peer` - The peer instance to run
/// * `shutdown_rx` - Watch receiver for shutdown signal
pub async fn spawn<L>(peer: Peer<L>, mut shutdown_rx: WatchReceiver<()>) -> Result<(), PeerError>
where
    L: crate::log::BucketLogProvider + Clone + Send + Sync + std::fmt::Debug + 'static,
    L::Error: std::fmt::Display + std::error::Error + Send + Sync + 'static,
{
    let node_id = peer.id();
    tracing::info!(peer_id = %node_id, "Starting peer");

    // Extract what we need for the router
    let inner_blobs = peer.blobs().protocol().clone();
    let endpoint = peer.endpoint().clone();
    let peer_for_router = peer.clone();

    // T-016b: gate raw iroh-blobs fetches by peer-type. Without this, an
    // Anonymous peer can bypass JAX-verb gating by hitting the iroh-blobs
    // ALPN directly. The wrapper rejects connections from peers who are not
    // Owner or Relay of any bucket this node hosts.
    let gated_blobs = GatedBlobsHandler {
        inner: inner_blobs,
        peer: peer.clone(),
    };

    // Build the protocol router with iroh-blobs and our custom protocol
    let router_builder = Router::builder(endpoint)
        .accept(zim_core::iroh::BLOBS_ALPN, gated_blobs)
        .accept(ALPN, peer_for_router);

    let router = router_builder.spawn();

    tracing::info!(peer_id = %node_id, "Peer protocol router started");

    // Wait for shutdown signal
    let _ = shutdown_rx.changed().await;
    tracing::info!(peer_id = %node_id, "Shutdown signal received, stopping peer");

    // Shutdown the router (this closes the endpoint and stops accepting connections)
    router
        .shutdown()
        .await
        .map_err(|e| PeerError::RouterShutdown(e.into()))?;

    tracing::info!(peer_id = %node_id, "Peer stopped");
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum PeerError {
    #[error("failed to shutdown router: {0}")]
    RouterShutdown(anyhow::Error),
}

/// Gate the raw iroh-blobs ALPN by peer-type.
///
/// T-016b's load-bearing security gate. JAX-verb gating in [`protocol`] is
/// moot if an attacker can bypass it by hitting `iroh_blobs::ALPN` directly,
/// so we wrap the inner [`BlobsProtocol`] handler with a peer-classification
/// step at accept-time. Connections from peers who are neither Owner nor
/// Relay of any bucket this node hosts are dropped.
///
/// **Known gap (deeper redesign — not in T-016b):** Relay peers currently
/// get full blob-fetch access for any blob this node holds, not just blobs
/// reachable from the requested bucket's `published_set`. iroh-blobs does
/// not expose per-request hooks for per-blob filtering at the protocol
/// layer; per-bucket-per-blob filtering would require either a custom
/// protocol on top of raw blob fetches or a fork of iroh-blobs that exposes
/// the hook. Tracked separately.
#[derive(Clone)]
struct GatedBlobsHandler<L: crate::log::BucketLogProvider> {
    inner: Arc<BlobsProtocol>,
    peer: Peer<L>,
}

impl<L> std::fmt::Debug for GatedBlobsHandler<L>
where
    L: crate::log::BucketLogProvider,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatedBlobsHandler").finish()
    }
}

impl<L> ProtocolHandler for GatedBlobsHandler<L>
where
    L: crate::log::BucketLogProvider + Clone + Send + Sync + std::fmt::Debug + 'static,
    L::Error: std::error::Error + Send + Sync + 'static,
{
    #[allow(refining_impl_trait)]
    fn accept(&self, conn: Connection) -> BoxFuture<'static, Result<(), AcceptError>> {
        let inner = self.inner.clone();
        let peer = self.peer.clone();
        Box::pin(async move {
            // Identify the connecting peer.
            let remote: PublicKey = match conn.remote_node_id() {
                Ok(id) => zim_core::iroh::from_iroh_public_key(&id),
                Err(e) => {
                    tracing::warn!("blob fetch rejected: could not read remote node id: {e}");
                    return Err(AcceptError::from(e));
                }
            };
            // Classify globally — at iroh-blobs ALPN-accept time we don't yet
            // know which bucket the peer will fetch from, so we use the broadest
            // role this peer holds across any bucket we host.
            let peer_type = peer.classify_remote_peer_global(&remote).await;
            match peer_type {
                PeerType::Member | PeerType::Relay => inner.accept(conn).await,
                PeerType::Anonymous => {
                    tracing::warn!(
                        "blob fetch rejected: peer {} is not a Member or Relay of any hosted bucket",
                        remote.to_hex()
                    );
                    // Drop the connection by returning Ok without delegating.
                    Ok(())
                }
            }
        })
    }
}
