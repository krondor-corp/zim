//! Vault-level tests.
//!
//! End-to-end exercise of `Vault::init` → `Vault::save` → `Vault::open`
//! against an in-memory `BlobStore` + in-memory `VaultLog`. No iroh,
//! no disk.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex as StdMutex};

use async_trait::async_trait;
use bytes::Bytes;
use zim_crypto::PrivateKey;

use crate::blobs::{BlobError, BlobStore};
use crate::fs::{AbsPath, FsError};
use crate::linked_data::{Hash, Link};

use super::log::{VaultLog, VaultLogError};
use super::{Vault, VaultError, VaultId};

// -- in-memory test fixtures --

#[derive(Clone, Default)]
struct MemBlobs(Arc<StdMutex<HashMap<Hash, Vec<u8>>>>);

#[async_trait]
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

#[derive(Debug, thiserror::Error)]
#[error("mem log error")]
struct MemLogError;

#[derive(Debug, Clone)]
struct MemEntry {
    current: Link,
    height: u64,
}

#[derive(Debug, Clone, Default)]
struct MemLog {
    inner: Arc<StdMutex<HashMap<VaultId, Vec<MemEntry>>>>,
}

#[async_trait]
impl VaultLog for MemLog {
    type Error = MemLogError;

    async fn exists(&self, id: VaultId) -> Result<bool, VaultLogError<Self::Error>> {
        Ok(self.inner.lock().unwrap().contains_key(&id))
    }

    async fn heads(
        &self,
        id: VaultId,
        height: u64,
    ) -> Result<Vec<Link>, VaultLogError<Self::Error>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get(&id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| e.height == height)
                    .map(|e| e.current.clone())
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn append(
        &self,
        id: VaultId,
        _name: String,
        current: Link,
        _previous: Option<Link>,
        height: u64,
    ) -> Result<(), VaultLogError<Self::Error>> {
        let mut inner = self.inner.lock().unwrap();
        let entries = inner.entry(id).or_default();
        if entries
            .iter()
            .any(|e| e.height == height && e.current == current)
        {
            return Err(VaultLogError::Conflict);
        }
        entries.push(MemEntry { current, height });
        Ok(())
    }

    async fn height(&self, id: VaultId) -> Result<u64, VaultLogError<Self::Error>> {
        self.inner
            .lock()
            .unwrap()
            .get(&id)
            .and_then(|entries| entries.iter().map(|e| e.height).max())
            .ok_or(VaultLogError::HeadNotFound(0))
    }

    async fn has(&self, id: VaultId, link: Link) -> Result<Vec<u64>, VaultLogError<Self::Error>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .get(&id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| e.current == link)
                    .map(|e| e.height)
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn list_vaults(&self) -> Result<Vec<VaultId>, VaultLogError<Self::Error>> {
        Ok(self.inner.lock().unwrap().keys().copied().collect())
    }
}

// -- tests --

#[tokio::test]
async fn init_then_open_roundtrips_content() {
    let blobs = MemBlobs::default();
    let log = MemLog::default();
    let owner = PrivateKey::generate();

    let mut vault = Vault::init("rt".to_string(), &owner, blobs.clone(), log.clone())
        .await
        .expect("init");
    let id = vault.id();

    let path = AbsPath::new("/readme.md").unwrap();
    vault
        .fs()
        .add(&path, Cursor::new(b"hello vault"))
        .await
        .unwrap();
    vault.save().await.expect("save");

    let reopened = Vault::open(id, blobs, log, &owner).await.expect("open");
    let bytes = reopened.fs().cat(&path).await.unwrap();
    assert_eq!(bytes, b"hello vault");
    assert_eq!(reopened.height(), 1);
}

#[tokio::test]
async fn save_increments_height_and_chains_previous() {
    let blobs = MemBlobs::default();
    let log = MemLog::default();
    let owner = PrivateKey::generate();

    let mut vault = Vault::init("rt".to_string(), &owner, blobs, log.clone())
        .await
        .expect("init");
    let id = vault.id();
    assert_eq!(vault.height(), 0);

    vault
        .fs()
        .mkdir(&AbsPath::new("/a").unwrap(), false)
        .await
        .unwrap();
    let _link1 = vault.save().await.expect("save 1");
    assert_eq!(vault.height(), 1);

    vault
        .fs()
        .mkdir(&AbsPath::new("/a/b").unwrap(), false)
        .await
        .unwrap();
    let _link2 = vault.save().await.expect("save 2");
    assert_eq!(vault.height(), 2);

    assert_eq!(log.height(id).await.unwrap(), 2);
}

#[tokio::test]
async fn history_walks_chain_backward() {
    let blobs = MemBlobs::default();
    let log = MemLog::default();
    let owner = PrivateKey::generate();

    let mut vault = Vault::init("rt".to_string(), &owner, blobs, log)
        .await
        .expect("init");
    for n in 1..=4 {
        let path = AbsPath::new(format!("/f{n}.txt")).unwrap();
        vault.fs().add(&path, Cursor::new(b"x")).await.unwrap();
        vault.save().await.unwrap();
    }

    let entries = vault.history(None, 10).await.unwrap();
    let heights: Vec<u64> = entries.iter().map(|e| e.height).collect();
    assert_eq!(heights, vec![4, 3, 2, 1, 0]);
}

#[tokio::test]
async fn identical_vaults_get_distinct_ids() {
    // Alice creates two vaults with the SAME name, SAME (empty)
    // content, SAME key. The genesis nonce must keep their derived
    // ids distinct — identity can't depend on content uniqueness.
    let blobs = MemBlobs::default();
    let log = MemLog::default();
    let alice = PrivateKey::generate();

    let v1 = Vault::init("demo".to_string(), &alice, blobs.clone(), log.clone())
        .await
        .expect("init v1");
    let v2 = Vault::init("demo".to_string(), &alice, blobs, log)
        .await
        .expect("init v2");

    assert_ne!(
        v1.id(),
        v2.id(),
        "identical-content vaults must not collide"
    );
}

#[tokio::test]
async fn vault_id_is_genesis_blob_hash() {
    let blobs = MemBlobs::default();
    let log = MemLog::default();
    let alice = PrivateKey::generate();

    let vault = Vault::init("demo".to_string(), &alice, blobs, log)
        .await
        .expect("init");

    // At genesis the manifest link IS the genesis link, so the id
    // must equal its hash. This is the self-certification anchor —
    // any chain can be verified against the id by walking to genesis.
    assert_eq!(
        vault.id(),
        VaultId::from_genesis_link(vault.manifest_link())
    );
}

#[tokio::test]
async fn open_errors_when_caller_has_no_share() {
    let blobs = MemBlobs::default();
    let log = MemLog::default();
    let alice = PrivateKey::generate();
    let bob = PrivateKey::generate();

    let alice_vault = Vault::init("shared".to_string(), &alice, blobs.clone(), log.clone())
        .await
        .expect("init");
    let id = alice_vault.id();

    let result: Result<Vault<MemBlobs, MemLog>, _> = Vault::open(id, blobs, log, &bob).await;
    match result {
        Err(VaultError::Fs(FsError::ShareNotFound)) => {}
        Err(other) => panic!("expected ShareNotFound, got {other:?}"),
        Ok(_) => panic!("expected ShareNotFound, got Ok"),
    }
}
