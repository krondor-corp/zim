//! Tree-only Fs tests.
//!
//! After the Vault-split refactor, `Fs` is a pure tree handle: it
//! decrypts + walks + mutates the file tree but doesn't carry the
//! manifest, the local peer's private key, or the chain log. Tests
//! that used to exercise `Fs::init` / `Fs::load` / `Fs::save` /
//! `Fs::publish` / `Fs::add_share` / etc. have been moved to
//! `crates/zim-core/src/vault/tests.rs` because they belong to the
//! `Vault` layer now.
//!
//! What remains in this file: tree mutations (`add`, `mkdir`, `rm`,
//! `mv`), reads (`cat`, `ls`, `get_entry_at_path`), and the CRDT
//! ops-log handoff (`apply_ops`). All of these are independent of
//! the manifest — they operate on the decrypted root dir, the
//! pending ops log, and the in-memory pin set.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use bytes::Bytes;

use crate::blobs::{BlobError, BlobStore};
use crate::linked_data::Hash;
use zim_crypto::{PrivateKey, Secret};

use super::abs_path::AbsPath;
use super::fs_inner::Fs;

/// In-memory blob store for testing. No disk, no iroh.
#[derive(Clone, Default)]
struct MemBlobs(Arc<StdMutex<HashMap<Hash, Vec<u8>>>>);

#[async_trait]
impl BlobStore for MemBlobs {
    async fn get(&self, hash: &Hash) -> Result<Bytes, BlobError> {
        let store = self.0.lock().unwrap();
        store
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
        mut reader: Box<dyn std::io::Read + Send + 'static>,
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

async fn setup() -> (Fs<MemBlobs>, PrivateKey) {
    let blobs = MemBlobs::default();
    let owner = PrivateKey::generate();
    let secret = Secret::generate();
    let (fs, _root_link) = Fs::init_tree(owner.public(), &secret, blobs)
        .await
        .expect("init_tree");
    (fs, owner)
}

#[tokio::test]
async fn alice_adds_a_file_and_reads_it_back() {
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/hello.txt").unwrap();

    fs.add(&path, Cursor::new(b"hello world")).await.unwrap();
    let leaf = fs.get_entry_at_path(&path).await.unwrap().unwrap();

    assert!(leaf.is_file());
}

#[tokio::test]
async fn add_records_blake3_of_plaintext_on_the_leaf() {
    // The load-bearing property for sync diff optimisation: the
    // `Entry::File` left behind by `add` carries `blake3(plaintext)`
    // so a sync engine can answer "did this file change?" against a
    // local copy without fetching or decrypting the ciphertext blob.
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/recipe.md").unwrap();
    let body = b"hash me before encryption";

    fs.add(&path, Cursor::new(body)).await.unwrap();
    let leaf = fs.get_entry_at_path(&path).await.unwrap().unwrap();

    let recorded = leaf
        .plaintext_hash()
        .expect("fresh writes populate plaintext_hash");
    let expected = blake3::hash(body);
    assert_eq!(
        recorded.as_bytes(),
        expected.as_bytes(),
        "plaintext_hash on Entry::File should equal blake3 of the input body"
    );
}

#[tokio::test]
async fn get_returns_same_leaf_for_repeated_lookups() {
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/data.json").unwrap();
    fs.add(&path, Cursor::new(b"{}")).await.unwrap();

    let first = fs.get_entry_at_path(&path).await.unwrap().unwrap();
    let second = fs.get_entry_at_path(&path).await.unwrap().unwrap();

    assert_eq!(first, second);
}

#[tokio::test]
async fn mkdir_then_add_inside_lists_the_file() {
    let (fs, _owner) = setup().await;
    fs.mkdir(&AbsPath::new("/docs").unwrap(), false)
        .await
        .unwrap();
    fs.add(
        &AbsPath::new("/docs/readme.md").unwrap(),
        Cursor::new(b"hello"),
    )
    .await
    .unwrap();

    let entries = fs.ls(&AbsPath::new("/docs").unwrap()).await.unwrap();
    assert_eq!(entries.len(), 1);
}

#[tokio::test]
async fn cat_returns_the_bytes_we_added() {
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/greeting.txt").unwrap();
    fs.add(&path, Cursor::new(b"hello from the test"))
        .await
        .unwrap();

    let bytes = fs.cat(&path).await.unwrap();
    assert_eq!(bytes, b"hello from the test");
}

#[tokio::test]
async fn rm_removes_the_file() {
    let (fs, _owner) = setup().await;
    let path = AbsPath::new("/doomed.txt").unwrap();
    fs.add(&path, Cursor::new(b"goodbye")).await.unwrap();

    fs.rm(&path).await.unwrap();
    let leaf = fs.get_entry_at_path(&path).await.unwrap();
    assert!(leaf.is_none(), "file should be gone after rm");
}

#[tokio::test]
async fn mv_moves_a_file_to_a_new_path() {
    let (fs, _owner) = setup().await;
    let src = AbsPath::new("/orig.txt").unwrap();
    let dst = AbsPath::new("/moved.txt").unwrap();
    fs.add(&src, Cursor::new(b"contents")).await.unwrap();

    fs.mv(&src, &dst).await.unwrap();

    assert!(fs.get_entry_at_path(&src).await.unwrap().is_none());
    let moved = fs.get_entry_at_path(&dst).await.unwrap().unwrap();
    assert!(moved.is_file());
    assert_eq!(fs.cat(&dst).await.unwrap(), b"contents");
}
