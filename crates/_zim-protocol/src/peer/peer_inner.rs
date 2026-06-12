use zim_core::blobs::BlobStore;
use zim_crypto::{PrivateKey, PublicKey};

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use uuid::Uuid;
use zim_core::iroh::{Endpoint, NodeId};

use zim_core::blobs::BlobsStore;

use super::PeerType;
use crate::log::BucketLogProvider;
use zim_core::fs::{Fs, FsError};
use zim_core::linked_data::Link;

use super::sync::{PingPeerJob, SyncJob, SyncProvider};

/// Overview of a peer's state, generic over a bucket log provider.
///  Provides everything that a peer needs in order to
///  load data, interact with peers, and manage buckets.
#[derive(Debug)]
pub struct Peer<L: BucketLogProvider> {
    log_provider: L,
    socket_address: SocketAddr,
    blobs_store: BlobsStore,
    secret_key: PrivateKey,
    endpoint: Endpoint,
    sync_provider: Arc<dyn SyncProvider<L>>,
}

impl<L: BucketLogProvider> Clone for Peer<L>
where
    L: Clone,
{
    fn clone(&self) -> Self {
        Self {
            log_provider: self.log_provider.clone(),
            socket_address: self.socket_address,
            blobs_store: self.blobs_store.clone(),
            secret_key: self.secret_key.clone(),
            endpoint: self.endpoint.clone(),
            sync_provider: self.sync_provider.clone(),
        }
    }
}

impl<L: BucketLogProvider> Peer<L> {
    pub(super) fn new(
        log_provider: L,
        socket_address: SocketAddr,
        blobs_store: BlobsStore,
        secret_key: PrivateKey,
        endpoint: Endpoint,
        sync_provider: Arc<dyn SyncProvider<L>>,
    ) -> Peer<L> {
        Self {
            log_provider,
            socket_address,
            blobs_store,
            secret_key,
            endpoint,
            sync_provider,
        }
    }

    pub fn logs(&self) -> &L {
        &self.log_provider
    }

    pub fn blobs(&self) -> &BlobsStore {
        &self.blobs_store
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn log_provider(&self) -> &L {
        &self.log_provider
    }

    pub fn secret(&self) -> &PrivateKey {
        &self.secret_key
    }

    pub fn socket(&self) -> &SocketAddr {
        &self.socket_address
    }

    pub fn id(&self) -> NodeId {
        self.endpoint.node_id()
    }

    pub async fn classify_remote_peer(&self, bucket_id: Uuid, remote: &PublicKey) -> PeerType {
        let head = match self.log_provider.head(bucket_id, None).await {
            Ok((link, _)) => link,
            Err(_) => return PeerType::Anonymous,
        };
        match self.blobs_store.get_cbor::<zim_core::fs::Manifest, _>(&head).await {
            Ok(manifest) => super::classify_peer(&manifest, remote),
            Err(_) => PeerType::Anonymous,
        }
    }

    /// Classify a remote peer globally — across all buckets this node hosts.
    ///
    /// Returns the broadest classification (`Owner` > `Mirror` > `Anonymous`)
    /// that the peer holds against *any* bucket this node knows about. Used
    /// at ALPN-accept time when the bucket context isn't yet known
    /// (e.g. raw iroh-blobs fetches).
    pub async fn classify_remote_peer_global(&self, remote: &PublicKey) -> PeerType {
        let bucket_ids = match self.log_provider.list_buckets().await {
            Ok(ids) => ids,
            Err(_) => return PeerType::Anonymous,
        };
        let mut best = PeerType::Anonymous;
        for bucket_id in bucket_ids {
            let cls = self.classify_remote_peer(bucket_id, remote).await;
            best = match (best, cls) {
                (PeerType::Member, _) | (_, PeerType::Member) => return PeerType::Member,
                (PeerType::Relay, _) | (_, PeerType::Relay) => PeerType::Relay,
                _ => PeerType::Anonymous,
            };
        }
        best
    }

    // ========================================
    // Sync Operations (dispatch to backend)
    // ========================================

    /// Dispatch a sync job to the sync provider
    ///
    /// The provider decides when/where this executes (immediately, queued, etc.)
    pub async fn dispatch(&self, job: SyncJob) -> Result<()>
    where
        L::Error: std::error::Error + Send + Sync + 'static,
    {
        self.sync_provider.execute(self, job).await
    }

    /// Ping all peers in a bucket's shares
    ///
    /// Dispatches ping jobs to all peers listed in the bucket's current
    /// manifest shares (except ourselves).
    pub async fn ping(&self, bucket_id: Uuid) -> Result<()>
    where
        L::Error: std::error::Error + Send + Sync + 'static,
    {
        // Get current head link
        let (head_link, _) = self
            .logs()
            .head(bucket_id, None)
            .await
            .map_err(|e| anyhow!("Failed to get head for bucket {}: {}", bucket_id, e))?;

        // Load manifest from blobs store
        let manifest: zim_core::fs::Manifest = self
            .blobs()
            .get_cbor(&head_link.hash())
            .await
            .map_err(|e| anyhow!("Failed to load manifest: {}", e))?;

        // Extract our own key to skip ourselves
        let our_key = self.secret().public().to_hex();

        // For each peer in shares, dispatch a ping job
        for peer_key_hex in manifest.shares().keys() {
            if peer_key_hex == &our_key {
                continue; // Skip ourselves
            }

            let peer_id = zim_crypto::PublicKey::from_hex(peer_key_hex)
                .map_err(|e| anyhow!("Invalid peer key in shares: {}", e))?;

            // Dispatch ping job
            if let Err(e) = self
                .dispatch(SyncJob::PingPeer(PingPeerJob { bucket_id, peer_id }))
                .await
            {
                tracing::warn!(
                    "Failed to dispatch ping to peer {} for bucket {}: {}",
                    peer_key_hex,
                    bucket_id,
                    e
                );
            }
        }

        Ok(())
    }

    /// Ping all peers for a bucket and collect their responses
    ///
    /// Returns a map of peer public key hex to their ping reply status.
    /// This waits for all pings to complete before returning.
    ///
    /// # Arguments
    ///
    /// * `bucket_id` - The bucket to ping peers for
    /// * `timeout` - Optional timeout duration for the entire operation
    pub async fn ping_and_collect(
        &self,
        bucket_id: Uuid,
        timeout: Option<std::time::Duration>,
    ) -> Result<std::collections::HashMap<String, crate::peer::protocol::PingReplyStatus>>
    where
        L::Error: std::error::Error + Send + Sync + 'static,
    {
        use crate::peer::protocol::bidirectional::BidirectionalHandler;
        use crate::peer::protocol::{Ping, PingMessage};

        // Get current head link
        let (head_link, head_height) = self
            .logs()
            .head(bucket_id, None)
            .await
            .map_err(|e| anyhow!("Failed to get head for bucket {}: {}", bucket_id, e))?;

        // Load manifest from blobs store
        let manifest: zim_core::fs::Manifest = self
            .blobs()
            .get_cbor(&head_link.hash())
            .await
            .map_err(|e| anyhow!("Failed to load manifest: {}", e))?;

        // Extract our own key to skip ourselves
        let our_key = self.secret().public().to_hex();

        // Collect all peer keys
        let peer_keys: Vec<_> = manifest
            .shares()
            .keys()
            .filter(|key| *key != &our_key)
            .cloned()
            .collect();

        // Ping all peers concurrently
        let mut tasks = Vec::new();

        for peer_key_hex in peer_keys {
            let peer_id = match zim_crypto::PublicKey::from_hex(&peer_key_hex) {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!("Invalid peer key {}: {}", peer_key_hex, e);
                    continue;
                }
            };

            let ping = PingMessage {
                bucket_id,
                link: head_link.clone(),
                height: head_height,
            };

            let peer = self.clone();
            let key = peer_key_hex.clone();

            tasks.push(tokio::spawn(async move {
                let result = Ping::send::<L>(&peer, &peer_id, ping).await;
                (key, result)
            }));
        }

        // Collect results with optional timeout
        let collect_future = async {
            let mut results: std::collections::HashMap<
                String,
                crate::peer::protocol::PingReplyStatus,
            > = std::collections::HashMap::new();
            for task in tasks {
                match task.await {
                    Ok((key, Ok(reply))) => {
                        results.insert(key, reply.status);
                    }
                    Ok((key, Err(e))) => {
                        tracing::warn!("Failed to ping peer {}: {}", key, e);
                    }
                    Err(e) => {
                        tracing::warn!("Task panicked: {}", e);
                    }
                }
            }
            Ok(results)
        };

        // Apply timeout if specified
        if let Some(timeout_duration) = timeout {
            match tokio::time::timeout(timeout_duration, collect_future).await {
                Ok(result) => result,
                Err(_) => Err(anyhow!(
                    "Ping collection timed out after {:?}",
                    timeout_duration
                )),
            }
        } else {
            collect_future.await
        }
    }

    /// Load mount at the current head of a bucket
    ///
    /// # Arguments
    ///
    /// * `bucket_id` - The UUID of the bucket to load
    ///
    /// # Returns
    ///
    /// The Fs at the current head of the bucket's log
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Bucket not found in log
    /// - Failed to load mount from blobs
    pub async fn mount(&self, bucket_id: Uuid) -> Result<Fs<BlobsStore>, FsError> {
        // Get current head link from log
        let (link, _height) = self
            .log_provider
            .head(bucket_id, None)
            .await
            .map_err(|e| FsError::Backing(anyhow::anyhow!("failed to get current head: {e}")))?;

        // Load mount at that link (height is read from manifest)
        Fs::load(&link, &self.secret_key, &self.blobs_store).await
    }

    /// Load mount for reading based on the peer's role in the bucket.
    ///
    /// This method determines the appropriate version to load based on the peer's role:
    /// - **Owners** see HEAD (latest state, including unpublished changes)
    /// - **Mirrors** (or unknown roles) see the latest_published version
    ///
    /// This ensures that mirrors only see content that has been explicitly published
    /// to them, while owners always see the most recent state.
    ///
    /// # Arguments
    ///
    /// * `bucket_id` - The UUID of the bucket to load
    ///
    /// # Returns
    ///
    /// The Fs at the appropriate version for this peer's role
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Bucket not found in log
    /// - No published version available (for mirrors)
    /// - Failed to load mount from blobs
    pub async fn mount_for_read(&self, bucket_id: Uuid) -> Result<Fs<BlobsStore>, FsError> {
        // Get current head link from log
        let (head_link, _) = self
            .log_provider
            .head(bucket_id, None)
            .await
            .map_err(|e| FsError::Backing(anyhow::anyhow!("failed to get current head: {e}")))?;

        // Classify ourselves against the HEAD manifest.
        let our_pubkey = self.secret_key.public();
        let peer_type = self.blobs_store.get_cbor::<zim_core::fs::Manifest, _>(&head_link)
            .await
            .ok()
            .map(|m| super::classify_peer(&m, &our_pubkey))
            .unwrap_or(PeerType::Anonymous);

        match peer_type {
            PeerType::Member => {
                // Members see HEAD (latest state).
                self.mount(bucket_id).await
            }
            PeerType::Relay | PeerType::Anonymous => {
                // Mirrors and anonymous peers see latest_published (read-only).
                let (link, _) = self
                    .log_provider
                    .latest_published(bucket_id)
                    .await
                    .map_err(|e| {
                        FsError::Backing(anyhow::anyhow!("failed to get latest published: {e}"))
                    })?
                    .ok_or_else(|| FsError::Backing(anyhow::anyhow!("no published version available")))?;
                Fs::load(&link, &self.secret_key, &self.blobs_store).await
            }
        }
    }

    /// Save a mount and append it to the bucket's log.
    ///
    /// The `publish` parameter controls publish state:
    /// - `None` — preserve current state
    /// - `Some(true)` — publish (expose secret so mirrors can decrypt)
    /// - `Some(false)` — unpublish (revoke mirror access)
    pub async fn save_mount(&self, mount: &Fs<BlobsStore>) -> Result<Link, FsError>
    where
        L::Error: std::error::Error + Send + Sync + 'static,
    {
        // Get our own public key to exclude from notifications
        let our_public_key = self.secret_key.public();
        tracing::info!("SAVE_MOUNT: Our public key: {}", our_public_key.to_hex());

        let inner_mount = mount.inner().await;
        let manifest = inner_mount.manifest();

        let bucket_id = *manifest.id();
        let name = manifest.name().to_string();

        // Get shares from the mount manifest
        let (link, previous_link, height) = mount.save().await?;
        let inner = mount.inner().await;
        let manifest = inner.manifest();
        let shares = manifest.shares();
        // Whole-bucket publish state is gone (T-008); per-file/folder
        // publication lives in `manifest.published_set()`. The log row's
        // `is_published` column is now always `false` for new entries; the
        // column is retained for historical reads of pre-T-008 buckets.
        let is_published = false;
        tracing::info!("SAVE_MOUNT: Found {} shares in manifest", shares.len());

        // Append to log
        self.log_provider
            .append(
                bucket_id,
                name,
                link.clone(),
                Some(previous_link),
                height,
                is_published,
            )
            .await
            .map_err(|e| FsError::Backing(anyhow::anyhow!("failed to append to log: {e}")))?;

        // Dispatch ping jobs for each peer (except ourselves)
        let mut notified_count = 0;
        for (peer_key_hex, _share) in shares.iter() {
            tracing::info!("SAVE_MOUNT: Checking share for peer: {}", peer_key_hex);

            // Parse the peer's public key
            if let Ok(peer_public_key) = PublicKey::from_hex(peer_key_hex) {
                // Skip ourselves
                if peer_public_key == our_public_key {
                    tracing::info!("SAVE_MOUNT: Skipping ourselves: {}", peer_key_hex);
                    continue;
                }

                tracing::info!(
                    "SAVE_MOUNT: Dispatching PingPeer job for bucket {} to peer {}",
                    bucket_id,
                    peer_key_hex
                );
                if let Err(e) = self
                    .dispatch(SyncJob::PingPeer(PingPeerJob {
                        bucket_id,
                        peer_id: peer_public_key,
                    }))
                    .await
                {
                    tracing::warn!("Failed to dispatch ping: {}", e);
                }
                notified_count += 1;
            } else {
                tracing::warn!(
                    "SAVE_MOUNT: Failed to parse peer public key: {}",
                    peer_key_hex
                );
            }
        }

        tracing::info!(
            "dispatched {} PingPeer jobs for bucket {}",
            notified_count,
            bucket_id
        );

        Ok(link)
    }
}
