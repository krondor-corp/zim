mod blob_store;
#[cfg(feature = "native")]
mod provider;

pub use blob_store::{BlobError, BlobStore};
#[cfg(feature = "native")]
pub use provider::BlobsProvider;
#[cfg(feature = "native")]
pub type BlobsStore = BlobsProvider;
#[cfg(feature = "native")]
pub const OUTBOARD_THRESHOLD: usize = 16 * 1024;
#[cfg(feature = "native")]
pub const IROH_BLOCK_SIZE: bao_tree::BlockSize = bao_tree::BlockSize::from_chunk_log(4);
#[cfg(feature = "native")]
pub type ApiClient = irpc::Client<iroh_blobs::api::proto::Request>;
