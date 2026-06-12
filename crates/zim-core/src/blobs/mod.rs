mod blob_store;
mod provider;

pub use blob_store::{BlobError, BlobStore};
pub use provider::BlobsProvider;
pub type BlobsStore = BlobsProvider;

pub const OUTBOARD_THRESHOLD: usize = 16 * 1024;
pub const IROH_BLOCK_SIZE: bao_tree::BlockSize = bao_tree::BlockSize::from_chunk_log(4);
pub type ApiClient = irpc::Client<iroh_blobs::api::proto::Request>;
