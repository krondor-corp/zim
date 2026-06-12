//! SQLite-indexed object-store blob backend (local dir or S3).
//!
//! A concrete store the daemon/hub can plug into
//! [`BlobsProvider`](zim_core::blobs::BlobsProvider) — same split as
//! [`SqliteVaultLog`](crate::log::SqliteVaultLog): `zim-core` owns
//! the `BlobStore` abstraction and the iroh-backed provider; this
//! crate owns the heavy concrete impls (rusqlite, the `object_store`
//! crate, S3 credentials).
//!
//! Bridged into iroh-blobs via [`actor::ObjectStoreActor`], which
//! speaks the iroh store `Command` protocol over an mpsc channel.
//! Construct through [`local_provider`] / [`s3_provider`].

use std::path::Path;

use anyhow::anyhow;
use zim_core::blobs::BlobsProvider;

pub(crate) mod actor;
mod database;
pub(crate) mod error;
mod storage;
pub(crate) mod store;

pub use actor::DEFAULT_MAX_IMPORT_SIZE;
pub use storage::{Storage, StorageConfig};
pub use store::ObjectStore;

/// `BlobsProvider` over a local-directory object store with a SQLite
/// index at `db_path`.
pub async fn local_provider(db_path: &Path, objects_path: &Path) -> anyhow::Result<BlobsProvider> {
    let store = ObjectStore::new_local(db_path, objects_path)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    Ok(provider_from(store))
}

/// `BlobsProvider` over an S3-compatible object store with a SQLite
/// index at `db_path`.
pub async fn s3_provider(
    db_path: &Path,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    bucket: &str,
    region: Option<&str>,
) -> anyhow::Result<BlobsProvider> {
    let config = StorageConfig::S3 {
        endpoint: endpoint.to_string(),
        access_key: access_key.to_string(),
        secret_key: secret_key.to_string(),
        bucket: bucket.to_string(),
        region: region.map(String::from),
    };
    let store = ObjectStore::new(db_path, config)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    Ok(provider_from(store))
}

/// Wire an [`ObjectStore`] into a [`BlobsProvider`]: spawn the actor
/// that translates iroh-blobs store `Command`s onto it, then hand the
/// command channel to iroh as a `Store`.
pub fn provider_from(store: ObjectStore) -> BlobsProvider {
    use iroh_blobs::api::proto::Command;

    let (tx, rx) = tokio::sync::mpsc::channel::<Command>(256);
    let actor = actor::ObjectStoreActor::new(store, rx, DEFAULT_MAX_IMPORT_SIZE);
    tokio::spawn(actor.run());
    let client: zim_core::blobs::ApiClient = tx.into();
    // SAFETY: `zim_core::iroh::Store` is a newtype over the same
    // irpc client type; iroh-blobs doesn't expose a public
    // constructor for it. Same transmute the impl used pre-move.
    let iroh_store: zim_core::iroh::Store =
        unsafe { std::mem::transmute::<zim_core::blobs::ApiClient, zim_core::iroh::Store>(client) };
    BlobsProvider::from(iroh_store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zim_core::blobs::BlobStore;

    #[tokio::test]
    async fn object_store_backs_a_blobs_provider() {
        let obj = ObjectStore::new_ephemeral().await.unwrap();
        let provider = provider_from(obj);
        let hash = BlobStore::put(&provider, b"world".to_vec()).await.unwrap();
        assert!(BlobStore::stat(&provider, &hash).await.unwrap());
    }
}
