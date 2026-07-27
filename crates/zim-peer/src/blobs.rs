//! [`BlobsProvider`] — the concrete blob store the daemon and hub run.
//!
//! One type, three backends, all constructed here:
//!
//! - [`BlobsProvider::memory`] — iroh's in-memory store (tests).
//! - [`BlobsProvider::local`]  — our SQLite-indexed local-directory
//!   object store ([`crate::object_store`]).
//! - [`BlobsProvider::s3`]     — the same object store over S3/minio.
//!
//! Whatever the backend, the provider wraps it in an
//! `Arc<BlobsProtocol>` so it serves the iroh blobs protocol to peers,
//! and implements [`BlobStore`] (get/put/stat) for the vault layer.
//! `zim-core` owns only the `BlobStore` trait; every concrete store
//! lives in this crate.

use std::future::IntoFuture;
use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use bytes::Bytes;

use crate::iroh::{
    ApiBlobStatus as BlobStatus, Blobs, BlobsProtocol, Downloader, Endpoint, MemStore, Shuffled,
};
use zim_core::linked_data::Hash;
use zim_crypto::PublicKey;

use zim_core::blobs::{BlobError, BlobStore};

/// Blobs larger than this get a precomputed bao outboard in the
/// object store; smaller ones recompute on the fly.
pub const OUTBOARD_THRESHOLD: usize = 16 * 1024;
/// iroh-blobs' chunk-group size — must match what iroh expects on the
/// wire.
pub const IROH_BLOCK_SIZE: bao_tree::BlockSize = bao_tree::BlockSize::from_chunk_log(4);

/// Concrete blob provider backed by `Arc<BlobsProtocol>`.
#[derive(Clone, Debug)]
pub struct BlobsProvider {
    protocol: Arc<BlobsProtocol>,
}

impl BlobsProvider {
    pub fn protocol(&self) -> &Arc<BlobsProtocol> {
        &self.protocol
    }

    pub fn blobs(&self) -> &Blobs {
        self.protocol.store().blobs()
    }

    /// iroh's in-memory store — tests and ephemeral peers.
    pub async fn memory() -> anyhow::Result<Self> {
        Ok(Self::from(MemStore::new()))
    }

    /// SQLite-indexed local-directory object store: index at
    /// `db_path`, blobs under `objects_path`.
    pub async fn local(db_path: &Path, objects_path: &Path) -> anyhow::Result<Self> {
        let store = crate::object_store::ObjectStore::new_local(db_path, objects_path)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        Ok(Self::from(crate::object_store::spawn_store(store)))
    }

    /// The same object store over an S3-compatible endpoint (minio in
    /// dev), index at `db_path`.
    #[allow(clippy::too_many_arguments)]
    pub async fn s3(
        db_path: &Path,
        endpoint: &str,
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        region: Option<&str>,
    ) -> anyhow::Result<Self> {
        let config = crate::object_store::StorageConfig::S3 {
            endpoint: endpoint.to_string(),
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            bucket: bucket.to_string(),
            region: region.map(String::from),
        };
        let store = crate::object_store::ObjectStore::new(db_path, config)
            .await
            .map_err(|e| anyhow!("{e}"))?;
        Ok(Self::from(crate::object_store::spawn_store(store)))
    }

    /// Promote a blob to a persistent (tagged) state so it survives GC.
    pub async fn tag(&self, hash: Hash) -> anyhow::Result<()> {
        let iroh_hash = crate::iroh::to_iroh_hash(hash);
        let haf = iroh_blobs::HashAndFormat::raw(iroh_hash);
        self.protocol
            .store()
            .tags()
            .create(haf)
            .await
            .map_err(|e| anyhow!("tag blob {hash}: {e}"))?;
        Ok(())
    }

    pub async fn download_hash(
        &self,
        hash: Hash,
        peer_ids: Vec<PublicKey>,
        endpoint: &Endpoint,
    ) -> anyhow::Result<()> {
        if self.stat(&hash).await.unwrap_or(false) {
            return Ok(());
        }
        let iroh_hash = crate::iroh::to_iroh_hash(hash);
        let downloader = Downloader::new(self.protocol.store(), endpoint);
        let discovery = Shuffled::new(
            peer_ids
                .iter()
                .map(crate::iroh::to_iroh_public_key)
                .collect(),
        );
        downloader.download(iroh_hash, discovery).await?;
        let haf = iroh_blobs::HashAndFormat::raw(iroh_hash);
        self.protocol
            .store()
            .tags()
            .create(haf)
            .await
            .map_err(|e| anyhow!("tag downloaded blob {hash}: {e}"))?;
        Ok(())
    }

    pub async fn download_hash_list(
        &self,
        hash: Hash,
        peer_ids: Vec<PublicKey>,
        endpoint: &Endpoint,
    ) -> anyhow::Result<()> {
        if self.stat(&hash).await.unwrap_or(false) {
            return Ok(());
        }
        let iroh_hash = crate::iroh::to_iroh_hash(hash);
        let downloader = Downloader::new(self.protocol.store(), endpoint);
        let discovery = Shuffled::new(
            peer_ids
                .iter()
                .map(crate::iroh::to_iroh_public_key)
                .collect(),
        );
        let hash_and_format =
            iroh_blobs::HashAndFormat::new(iroh_hash, iroh_blobs::BlobFormat::HashSeq);
        downloader.download(hash_and_format, discovery).await?;
        self.protocol
            .store()
            .tags()
            .create(hash_and_format)
            .await
            .map_err(|e| anyhow!("tag downloaded hashseq {hash}: {e}"))?;
        Ok(())
    }
}

impl From<MemStore> for BlobsProvider {
    fn from(store: MemStore) -> Self {
        Self {
            protocol: Arc::new(BlobsProtocol::new(&store, None)),
        }
    }
}

impl From<crate::iroh::Store> for BlobsProvider {
    fn from(store: crate::iroh::Store) -> Self {
        Self {
            protocol: Arc::new(BlobsProtocol::new(&store, None)),
        }
    }
}

#[async_trait::async_trait]
impl BlobStore for BlobsProvider {
    async fn get(&self, hash: &Hash) -> Result<Bytes, BlobError> {
        let iroh_hash = crate::iroh::to_iroh_hash(*hash);
        self.blobs()
            .get_bytes(iroh_hash)
            .await
            .map_err(|e| anyhow!("get: {e}").into())
    }

    async fn put(&self, data: Vec<u8>) -> Result<Hash, BlobError> {
        let iroh_hash = self
            .blobs()
            .add_bytes(data)
            .into_future()
            .await
            .map_err(|e| anyhow!("put: {e}"))?
            .hash;
        Ok(crate::iroh::from_iroh_hash(iroh_hash))
    }

    async fn put_reader(
        &self,
        mut reader: Box<dyn std::io::Read + Send + 'static>,
    ) -> Result<Hash, BlobError> {
        const CHUNK: usize = 64 * 1024;
        let (tx, rx) = flume::bounded::<std::io::Result<Bytes>>(4);
        tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; CHUNK];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(Ok(Bytes::copy_from_slice(&buf[..n]))).is_err() {
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                }
            }
        });
        let stream = rx.into_stream();
        let iroh_hash = self
            .blobs()
            .add_stream(stream)
            .await
            .await
            .map_err(|e| anyhow!("put_reader: {e}"))?
            .hash;
        Ok(crate::iroh::from_iroh_hash(iroh_hash))
    }

    async fn stat(&self, hash: &Hash) -> Result<bool, BlobError> {
        let iroh_hash = crate::iroh::to_iroh_hash(*hash);
        let stat = self
            .blobs()
            .status(iroh_hash)
            .await
            .map_err(|e| anyhow!("stat: {e}"))?;
        Ok(matches!(stat, BlobStatus::Complete { .. }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_from_mem_store() {
        let provider = BlobsProvider::from(MemStore::new());
        let hash = BlobStore::put(&provider, b"hello".to_vec()).await.unwrap();
        let data = BlobStore::get(&provider, &hash).await.unwrap();
        assert_eq!(data.as_ref(), b"hello");
    }
}
