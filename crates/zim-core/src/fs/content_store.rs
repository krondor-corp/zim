//! Content store: the fs-layer's view of storage.
//!
//! Blobs speak blobs; the content store speaks fs. Every method here
//! takes or returns [`Dir`], [`Entry`], or [`Link`] — never raw bytes
//! and a hash. Encryption and codec lives inside.
//!
//! Two storage destinations sit behind the same type:
//!
//! - **Metadata pack** (in-memory): encrypted dir bodies, snapshotted
//!   inline into the next manifest. [`ContentStore::put_metadata`]
//!   stages, [`ContentStore::get_metadata`] reads (tiered — checks the
//!   pack first, then falls through to the inner store for older bodies
//!   referenced from prior manifests).
//! - **Inner blob store**: encrypted file content.
//!   [`ContentStore::put_file`] streams in via the per-file secret;
//!   [`ContentStore::get_file`] streams out.
//!
//! Anything that isn't fs-shaped — the manifest itself (signed, not
//! encrypted), the ops-log blob, manifest history reads — goes through
//! [`ContentStore::inner`] explicitly. The verbosity is the point: it
//! flags "I'm crossing the layer boundary here."

use std::collections::BTreeMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::blobs::{BlobError, BlobStore};
use crate::linked_data::{BlockEncoded, CodecError, Hash, Link, LD_RAW_CODEC};
use zim_crypto::{Secret, SecretError};

use super::entry::Dir;

/// On-disk format: a map from content-hash to encrypted dir-body bytes.
/// Serialized as DAG-CBOR and stored inline inside every manifest.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata(BTreeMap<Hash, Vec<u8>>);

impl Metadata {
    /// Create an empty metadata pack.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Insert (or overwrite) the encrypted bytes stored under `hash`.
    pub fn insert(&mut self, hash: Hash, encrypted_bytes: Vec<u8>) {
        self.0.insert(hash, encrypted_bytes);
    }

    /// Look up the encrypted bytes stored under `hash`.
    pub fn get(&self, hash: &Hash) -> Option<&Vec<u8>> {
        self.0.get(hash)
    }

    /// True if `hash` is present in the pack.
    pub fn contains(&self, hash: &Hash) -> bool {
        self.0.contains_key(hash)
    }

    /// Remove and return the bytes stored under `hash`.
    pub fn remove(&mut self, hash: &Hash) -> Option<Vec<u8>> {
        self.0.remove(hash)
    }

    /// Number of entries in the pack.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True if the pack contains no entries.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterate over the hashes present in the pack.
    pub fn hashes(&self) -> impl Iterator<Item = &Hash> {
        self.0.keys()
    }
}

/// Errors emitted by the [`ContentStore`] API — transparent
/// passthroughs of lower-level errors. The store reads by `(link,
/// key)`; which [`Entry`] variant those came from is the caller's
/// business, decided by a `match` and not at runtime here.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    /// The inner blob store failed (network, I/O, missing blob).
    #[error("blob: {0}")]
    Blob(#[from] BlobError),
    /// Encryption or decryption failed.
    #[error("secret: {0}")]
    Secret(#[from] SecretError),
    /// DAG-CBOR encode/decode failed.
    #[error("codec: {0}")]
    Codec(#[from] CodecError),
}

/// Holds the in-memory metadata pack and a handle to the underlying
/// blob store. The fs layer talks to this; blob-shaped operations are
/// reached through [`Self::inner`].
#[derive(Clone)]
pub struct ContentStore<B: BlobStore> {
    metadata: Arc<Mutex<Metadata>>,
    inner: B,
}

/// Tee adapter for [`ContentStore::put_file`]: forwards every plaintext
/// byte to both the encrypt-and-store pipeline and a shared
/// `blake3::Hasher`. The hasher is wrapped in `Arc<Mutex<_>>` because
/// `encrypt_reader` consumes the wrapper by move; the outer scope
/// keeps the other handle to call `finalize` after the stream drains.
struct HashingReader {
    inner: Box<dyn Read + Send + 'static>,
    hasher: Arc<Mutex<blake3::Hasher>>,
}

impl Read for HashingReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.hasher.lock().unwrap().update(&buf[..n]);
        }
        Ok(n)
    }
}

impl<B: BlobStore> ContentStore<B> {
    /// Wrap `inner` with an initial `metadata` pack. The pack is the one
    /// just decoded from a manifest (on load) or freshly constructed (on
    /// init); subsequent dir-body writes mutate it via
    /// [`Self::put_metadata`].
    pub fn new(inner: B, metadata: Metadata) -> Self {
        Self {
            metadata: Arc::new(Mutex::new(metadata)),
            inner,
        }
    }

    /// Reach the underlying blob store. Use this for anything that isn't
    /// fs-shaped: the manifest blob, the ops-log ciphertext, manifest
    /// history reads.
    pub fn inner(&self) -> &B {
        &self.inner
    }

    // ─── Metadata pack ────────────────────────────────────────────────

    /// Encode + encrypt `dir`, stage the ciphertext in the metadata pack,
    /// and return the [`Entry`] a parent would store in its children map.
    pub fn put_metadata(&self, secret: &Secret, dir: &Dir) -> Result<Link, ContentError> {
        let plaintext = dir.encode()?;
        let encrypted = secret.encrypt(&plaintext)?;
        let hash = Hash::new(&encrypted);
        self.metadata.lock().unwrap().insert(hash, encrypted);
        Ok(Link::new(LD_RAW_CODEC, hash))
    }

    /// Fetch + decrypt + decode the dir body at `link` under `secret`.
    /// Tiered: checks the metadata pack first, falls through to the
    /// inner store for bodies referenced from older manifests. For a
    /// child dir the caller matched an [`Entry::Dir`] and passes its
    /// link and `ratchet.key()`; for the root, the manifest's link and
    /// the vault's root key.
    pub async fn get_dir(&self, link: &Link, secret: &Secret) -> Result<Dir, ContentError> {
        let ciphertext = self.get_metadata_bytes(&link.hash()).await?;
        let plaintext = secret.decrypt(&ciphertext)?;
        Ok(Dir::decode(&plaintext)?)
    }

    /// Snapshot the metadata for serialization at save time.
    pub fn snapshot_metadata(&self) -> Metadata {
        self.metadata.lock().unwrap().clone()
    }

    /// Replace the metadata (after loading a pack on mount).
    pub fn load_metadata(&self, metadata: Metadata) {
        *self.metadata.lock().unwrap() = metadata;
    }

    /// Drop a single entry by hash. Called eagerly from
    /// `set_entry_at_path` (rebuilt ancestor), `rm` (removed subtree),
    /// and `save` (prior root).
    pub fn evict(&self, hash: &Hash) {
        self.metadata.lock().unwrap().remove(hash);
    }

    /// Drop a batch of orphan hashes in one lock acquisition.
    pub fn evict_many<'a, I: IntoIterator<Item = &'a Hash>>(&self, hashes: I) {
        let mut metadata = self.metadata.lock().unwrap();
        for hash in hashes {
            metadata.remove(hash);
        }
    }

    // ─── File content ─────────────────────────────────────────────────

    /// Stream encrypted file content into the inner blob store.
    /// Returns the raw-codec [`Link`] for the ciphertext **and** the
    /// `blake3(plaintext)` hash — computed in one pass by tee-ing
    /// the plaintext reader into a `blake3::Hasher` before it hits
    /// the encryption layer. Caller wraps the `Link` into an
    /// [`Entry::File`] alongside the plaintext hash so sync diffing
    /// can short-circuit on unchanged files.
    pub async fn put_file(
        &self,
        secret: &Secret,
        reader: Box<dyn Read + Send + 'static>,
    ) -> Result<(Link, Hash), ContentError> {
        let hasher: Arc<Mutex<blake3::Hasher>> = Arc::new(Mutex::new(blake3::Hasher::new()));
        let tee = HashingReader {
            inner: reader,
            hasher: hasher.clone(),
        };
        let encrypted_reader = secret.encrypt_reader(tee)?;
        let ct_hash = self.inner.put_reader(Box::new(encrypted_reader)).await?;
        // `encrypt_reader` consumed every byte of the tee, so the
        // hasher saw the whole plaintext.
        let pt_hash_bytes: [u8; 32] = *hasher.lock().unwrap().finalize().as_bytes();
        let pt_hash = Hash::from_bytes(pt_hash_bytes);
        Ok((Link::new(LD_RAW_CODEC, ct_hash), pt_hash))
    }

    /// Stream decrypted file content out of the inner blob store. The
    /// fetch itself is buffered today (the [`BlobStore`] trait has no
    /// streaming `get`); decryption is genuinely streaming on top.
    pub async fn get_file(
        &self,
        link: &Link,
        secret: &Secret,
    ) -> Result<Box<dyn Read + Send>, ContentError> {
        let ciphertext = self.inner.get(&link.hash()).await?;
        let reader = secret.decrypt_reader(std::io::Cursor::new(ciphertext.to_vec()))?;
        Ok(Box::new(reader))
    }

    // ─── Internal ─────────────────────────────────────────────────────

    /// Pack-first byte lookup for dir bodies. Private — callers go
    /// through `get_dir`.
    async fn get_metadata_bytes(&self, hash: &Hash) -> Result<Vec<u8>, BlobError> {
        if let Some(cached) = self.metadata.lock().unwrap().get(hash).cloned() {
            return Ok(cached);
        }
        Ok(self.inner.get(hash).await?.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Entry;
    use bytes::Bytes;
    use std::io::Cursor;
    use std::path::Path;
    use zim_crypto::EntryRatchet;

    #[derive(Clone, Default)]
    struct MemBlobs(Arc<Mutex<BTreeMap<Hash, Vec<u8>>>>);

    #[async_trait::async_trait]
    impl BlobStore for MemBlobs {
        async fn get(&self, hash: &Hash) -> Result<Bytes, BlobError> {
            self.0
                .lock()
                .unwrap()
                .get(hash)
                .map(|v| Bytes::from(v.clone()))
                .ok_or_else(|| BlobError::NotFound(*hash))
        }

        async fn put(&self, data: Vec<u8>) -> Result<Hash, BlobError> {
            let hash = Hash::new(&data);
            self.0.lock().unwrap().insert(hash, data);
            Ok(hash)
        }

        async fn put_reader(
            &self,
            mut reader: Box<dyn Read + Send + 'static>,
        ) -> Result<Hash, BlobError> {
            let mut buf = Vec::new();
            reader
                .read_to_end(&mut buf)
                .map_err(|e| BlobError::Store(e.into()))?;
            self.put(buf).await
        }

        async fn stat(&self, hash: &Hash) -> Result<bool, BlobError> {
            Ok(self.0.lock().unwrap().contains_key(hash))
        }
    }

    #[tokio::test]
    async fn put_then_get_metadata_round_trip() {
        let ratchet = EntryRatchet::seed();
        let secret = ratchet.key();
        let mut dir = Dir::new();
        dir.insert(
            "child".to_string(),
            Entry::file(
                Link::default(),
                EntryRatchet::seed(),
                None,
                Path::new("example"),
                Hash::new(b""),
            ),
        );

        let store = ContentStore::new(MemBlobs::default(), Metadata::new());
        let link = store.put_metadata(&secret, &dir).unwrap();

        let round_tripped = store.get_dir(&link, &secret).await.unwrap();
        assert_eq!(round_tripped, dir);
    }

    #[tokio::test]
    async fn get_dir_falls_through_to_inner_on_pack_miss() {
        let secret = EntryRatchet::seed().key();
        let mut dir = Dir::new();
        dir.insert(
            "x".to_string(),
            Entry::file(
                Link::default(),
                EntryRatchet::seed(),
                None,
                Path::new("example"),
                Hash::new(b""),
            ),
        );
        let plaintext = dir.encode().unwrap();
        let ciphertext = secret.encrypt(&plaintext).unwrap();
        let hash = Hash::new(&ciphertext);

        let inner = MemBlobs::default();
        inner.put(ciphertext).await.unwrap();
        let store = ContentStore::new(inner, Metadata::new());

        let round_tripped = store
            .get_dir(&Link::new(LD_RAW_CODEC, hash), &secret)
            .await
            .unwrap();
        assert_eq!(round_tripped, dir);
    }

    #[tokio::test]
    async fn put_then_get_file_round_trip() {
        let secret = EntryRatchet::seed().key();
        let store = ContentStore::new(MemBlobs::default(), Metadata::new());

        let plaintext = b"hello, encrypted world";
        let (link, pt_hash) = store
            .put_file(&secret, Box::new(Cursor::new(plaintext.to_vec())))
            .await
            .unwrap();

        // `put_file` returns blake3(plaintext) alongside the ciphertext
        // link — sanity-check it before exercising the round trip.
        assert_eq!(pt_hash.as_bytes(), blake3::hash(plaintext).as_bytes());

        let mut reader = store.get_file(&link, &secret).await.unwrap();
        let mut got = Vec::new();
        reader.read_to_end(&mut got).unwrap();
        assert_eq!(got.as_slice(), plaintext);
    }

    #[tokio::test]
    async fn put_file_does_not_touch_pack() {
        let secret = Secret::generate();
        let inner = MemBlobs::default();
        let store = ContentStore::new(inner.clone(), Metadata::new());

        let (link, _) = store
            .put_file(&secret, Box::new(Cursor::new(b"file".to_vec())))
            .await
            .unwrap();

        assert!(inner.stat(&link.hash()).await.unwrap());
        assert!(!store.snapshot_metadata().contains(&link.hash()));
    }

    #[tokio::test]
    async fn evict_drops_single_entry() {
        let secret = Secret::generate();
        let dir = Dir::new();
        let store = ContentStore::new(MemBlobs::default(), Metadata::new());
        let link = store.put_metadata(&secret, &dir).unwrap();

        store.evict(&link.hash());
        assert!(!store.snapshot_metadata().contains(&link.hash()));
    }

    #[tokio::test]
    async fn metadata_round_trips_through_cbor() {
        let mut metadata = Metadata::new();
        metadata.insert(Hash::new(b"a"), b"block_a".to_vec());
        metadata.insert(Hash::new(b"b"), b"block_b".to_vec());

        let encoded = metadata.encode().unwrap();
        let decoded = Metadata::decode(&encoded).unwrap();
        assert_eq!(metadata, decoded);
    }
}
