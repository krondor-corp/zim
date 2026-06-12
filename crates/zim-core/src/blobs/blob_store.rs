use async_trait::async_trait;
use bytes::Bytes;

use crate::linked_data::{BlockEncoded, CodecError, Hash, Link, LD_CBOR_CODEC, LD_RAW_CODEC};

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    #[error("blob not found: {0}")]
    NotFound(Hash),
    #[error("store: {0}")]
    Store(#[from] anyhow::Error),
    #[error("decode error: {0}")]
    Decode(#[from] CodecError),
}

#[async_trait]
pub trait BlobStore: Clone + Send + Sync + 'static {
    /// Primitive operations. Implementors only need to deal with `Hash`.
    async fn get(&self, hash: &Hash) -> Result<Bytes, BlobError>;
    async fn put(&self, data: Vec<u8>) -> Result<Hash, BlobError>;
    async fn stat(&self, hash: &Hash) -> Result<bool, BlobError>;

    /// Stream bytes from a `Read` into the store without buffering the whole
    /// payload. Required — every implementor handles streaming on its own
    /// terms (iroh-blobs has a native chunked stream API; in-memory test
    /// stores can collect).
    async fn put_reader(
        &self,
        reader: Box<dyn std::io::Read + Send + 'static>,
    ) -> Result<Hash, BlobError>;

    /// Put opaque bytes. Returns a raw-codec Link.
    async fn put_raw(&self, data: Vec<u8>) -> Result<Link, BlobError> {
        let hash = self.put(data).await?;
        Ok(Link::new(LD_RAW_CODEC, hash))
    }

    /// Put a CBOR-encodable value. Returns a dag-cbor-codec Link.
    async fn put_cbor<T: BlockEncoded + Sync>(&self, value: &T) -> Result<Link, BlobError> {
        let bytes = value.encode()?;
        let hash = self.put(bytes).await?;
        Ok(Link::new(LD_CBOR_CODEC, hash))
    }

    /// Get a CBOR-encodable value by anything that converts to a `Hash`
    /// (e.g. `&Hash`, `&Link`). The conversion happens at the call boundary,
    /// so the underlying `get` still takes `&Hash`.
    async fn get_cbor<T, K>(&self, key: K) -> Result<T, BlobError>
    where
        T: BlockEncoded + Send,
        K: Into<Hash> + Send,
    {
        let hash: Hash = key.into();
        let bytes = self.get(&hash).await?;
        Ok(T::decode(&bytes)?)
    }
}
