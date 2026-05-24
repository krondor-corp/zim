//! Zim storage layer: content-addressed blob store + iroh-blobs networking wrapper.
//!
//! - **Backend**: SQLite metadata + pluggable object storage (S3/MinIO/local/memory).
//! - **Content addressing**: BLAKE3 hashes, CIDs, codecs (DAG-CBOR + raw), via `linked_data`.
//! - **iroh-blobs interface**: `BlobsStore` wraps `iroh_blobs::BlobsProtocol` for serving blobs over the network.

mod actor;
mod blobs_store;
mod database;
mod error;
pub mod linked_data;
mod object_store;
mod storage;

pub use actor::DEFAULT_MAX_IMPORT_SIZE;
pub use blobs_store::{BlobsStore, BlobsStoreError};
pub use error::{BlobStoreError, Result};
pub use object_store::ObjectStore;
pub use storage::{ObjectStoreConfig, Storage};
