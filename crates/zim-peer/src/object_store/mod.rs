//! SQLite-indexed object-store blob backend (local dir or S3).
//!
//! The backend behind [`BlobsProvider::local`] /
//! [`BlobsProvider::s3`](crate::blobs::BlobsProvider::s3) — construct
//! through those, not directly. Bridged into iroh-blobs via
//! [`actor::ObjectStoreActor`], which speaks the iroh store `Command`
//! protocol over an mpsc channel ([`spawn_store`]).
//!
//! [`BlobsProvider::local`]: crate::blobs::BlobsProvider::local

pub(crate) mod actor;
mod database;
pub(crate) mod error;
mod storage;
pub(crate) mod store;

pub use actor::DEFAULT_MAX_IMPORT_SIZE;
pub use storage::{Storage, StorageConfig};
pub use store::ObjectStore;

/// Wire an [`ObjectStore`] into iroh: spawn the actor that translates
/// iroh-blobs store `Command`s onto it, then hand the command channel
/// back as a `Store` for [`crate::blobs::BlobsProvider`] to wrap.
pub(crate) fn spawn_store(store: ObjectStore) -> crate::iroh::Store {
    use iroh_blobs::api::proto::Command;

    type ApiClient = irpc::Client<iroh_blobs::api::proto::Request>;

    let (tx, rx) = tokio::sync::mpsc::channel::<Command>(256);
    let actor = actor::ObjectStoreActor::new(store, rx, DEFAULT_MAX_IMPORT_SIZE);
    tokio::spawn(actor.run());
    let client: ApiClient = tx.into();
    // SAFETY: `crate::iroh::Store` is a newtype over the same
    // irpc client type; iroh-blobs doesn't expose a public
    // constructor for it. Same transmute the impl used pre-move.
    unsafe { std::mem::transmute::<ApiClient, crate::iroh::Store>(client) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zim_core::blobs::BlobStore;

    #[tokio::test]
    async fn object_store_backs_a_blobs_provider() {
        let obj = ObjectStore::new_ephemeral().await.unwrap();
        let provider = crate::blobs::BlobsProvider::from(spawn_store(obj));
        let hash = BlobStore::put(&provider, b"world".to_vec()).await.unwrap();
        assert!(BlobStore::stat(&provider, &hash).await.unwrap());
    }
}
