use std::future::IntoFuture;
use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use bytes::Bytes;

use crate::iroh::{
    ApiBlobStatus as BlobStatus, Blobs, BlobsProtocol, Downloader, Endpoint, FsStore, Hash,
    MemStore, Shuffled,
};
use zim_crypto::PublicKey;

use super::blob_store::{BlobError, BlobStore};

/// Concrete blob provider backed by `Arc<BlobsProtocol>`.
///
/// Created from any iroh-compatible store backend:
/// - `MemStore` (iroh in-memory)
/// - `FsStore` (iroh filesystem)
/// - `crate::iroh::Store` (any iroh store — `zim_peer::object_store`
///   plugs its SQLite+S3 backend in through this)
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

    pub async fn memory() -> anyhow::Result<Self> {
        Ok(Self::from(MemStore::new()))
    }

    pub async fn legacy_fs(path: &Path) -> anyhow::Result<Self> {
        let store = FsStore::load(path).await?;
        Ok(Self::from(store))
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
        let downloader = Downloader::new(self.protocol.store(), endpoint);
        let discovery = Shuffled::new(
            peer_ids
                .iter()
                .map(crate::iroh::to_iroh_public_key)
                .collect(),
        );
        downloader.download(hash, discovery).await?;
        // Pin the freshly-downloaded blob behind a persistent tag.
        // Without this, iroh-blobs treats the download as
        // ephemeral — the underlying file survives only as long as
        // the temp-tag the downloader created, and the next gc
        // sweep (including daemon restart) reclaims it. That's how
        // remote manifests learned via apply_chain went missing on
        // restart even though the log still pointed at them.
        let haf = iroh_blobs::HashAndFormat::raw(hash);
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
        let downloader = Downloader::new(self.protocol.store(), endpoint);
        let discovery = Shuffled::new(
            peer_ids
                .iter()
                .map(crate::iroh::to_iroh_public_key)
                .collect(),
        );
        let hash_and_format =
            crate::iroh::HashAndFormat::new(hash, crate::iroh::BlobFormat::HashSeq);
        downloader.download(hash_and_format, discovery).await?;
        // Persistent tag — see `download_hash` for the rationale.
        self.protocol
            .store()
            .tags()
            .create(hash_and_format)
            .await
            .map_err(|e| anyhow!("tag downloaded hashseq {hash}: {e}"))?;
        Ok(())
    }
}

// -- From impls for each store backend --

impl From<MemStore> for BlobsProvider {
    fn from(store: MemStore) -> Self {
        Self {
            protocol: Arc::new(BlobsProtocol::new(&store, None)),
        }
    }
}

impl From<FsStore> for BlobsProvider {
    fn from(store: FsStore) -> Self {
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
        self.blobs()
            .get_bytes(*hash)
            .await
            .map_err(|e| anyhow!("get: {e}").into())
    }

    async fn put(&self, data: Vec<u8>) -> Result<Hash, BlobError> {
        let hash = self
            .blobs()
            .add_bytes(data)
            .into_future()
            .await
            .map_err(|e| anyhow!("put: {e}"))?
            .hash;
        Ok(hash)
    }

    /// Streams the reader into iroh-blobs' `add_stream`. Chunks the sync
    /// reader on a blocking thread (64KB chunks) into a flume-backed
    /// `Stream<Item = io::Result<Bytes>>`. Only one chunk is in flight at
    /// a time — memory footprint is bounded regardless of payload size.
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
        let hash = self
            .blobs()
            .add_stream(stream)
            .await
            .await
            .map_err(|e| anyhow!("put_reader: {e}"))?
            .hash;
        Ok(hash)
    }

    async fn stat(&self, hash: &Hash) -> Result<bool, BlobError> {
        let stat = self
            .blobs()
            .status(*hash)
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
