use std::path::Path;

use bytes::Bytes;
use tracing::{debug, info, warn};

use zim_core::blobs::{IROH_BLOCK_SIZE, OUTBOARD_THRESHOLD};
use zim_core::iroh::Hash;

use super::database::{BlobMetadata, BlobState, Database};
use super::error::Result;
use super::storage::{Storage, StorageConfig};

#[derive(Debug, Clone)]
pub struct ObjectStore {
    pub(crate) db: Database,
    pub(crate) storage: Storage,
}

impl ObjectStore {
    pub async fn new(db_path: &Path, config: StorageConfig) -> Result<Self> {
        let db = Database::new(db_path)?;
        let storage = Storage::new(config).await?;
        Ok(Self { db, storage })
    }

    #[allow(dead_code)]
    pub async fn in_memory(config: StorageConfig) -> Result<Self> {
        let db = Database::in_memory()?;
        let storage = Storage::new(config).await?;
        Ok(Self { db, storage })
    }

    pub async fn new_local(db_path: &Path, objects_path: &Path) -> Result<Self> {
        Self::new(
            db_path,
            StorageConfig::Local {
                path: objects_path.to_path_buf(),
            },
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn new_ephemeral() -> Result<Self> {
        Self::in_memory(StorageConfig::Memory).await
    }

    fn data_key(hash: &str) -> String {
        format!("data/{hash}")
    }

    fn outboard_key(hash: &str) -> String {
        format!("outboard/{hash}")
    }

    pub async fn put(&self, data: Vec<u8>) -> Result<Hash> {
        let size = data.len();
        let hash = Hash::new(&data);
        let hash_str = hash.to_string();
        debug!(hash = %hash_str, size = size, "storing blob");
        let has_outboard = size > OUTBOARD_THRESHOLD;
        if has_outboard {
            let outboard =
                bao_tree::io::outboard::PreOrderMemOutboard::create(&data, IROH_BLOCK_SIZE);
            self.storage
                .put(&Self::outboard_key(&hash_str), Bytes::from(outboard.data))
                .await?;
        }
        self.storage
            .put(&Self::data_key(&hash_str), Bytes::from(data))
            .await?;
        BlobMetadata::insert(&self.db, &hash_str, size as i64, has_outboard)?;
        info!(hash = %hash_str, size = size, "blob stored successfully");
        Ok(hash)
    }

    pub async fn get(&self, hash: &Hash) -> Result<Option<Bytes>> {
        let hash_str = hash.to_string();
        if !BlobMetadata::exists(&self.db, &hash_str)? {
            return Ok(None);
        }
        self.storage.get(&Self::data_key(&hash_str)).await
    }

    pub async fn delete(&self, hash: &Hash) -> Result<bool> {
        let hash_str = hash.to_string();
        let metadata = BlobMetadata::get(&self.db, &hash_str)?;
        let Some(metadata) = metadata else {
            return Ok(false);
        };
        self.storage.delete(&Self::data_key(&hash_str)).await?;
        if metadata.has_outboard {
            self.storage.delete(&Self::outboard_key(&hash_str)).await?;
        }
        BlobMetadata::delete(&self.db, &hash_str)?;
        info!(hash = %hash_str, "blob deleted");
        Ok(true)
    }

    pub async fn put_with_outboard(&self, data: Vec<u8>, outboard: Vec<u8>) -> Result<Hash> {
        let size = data.len();
        let hash = Hash::new(&data);
        let hash_str = hash.to_string();
        let has_outboard = !outboard.is_empty();
        self.storage
            .put(&Self::data_key(&hash_str), Bytes::from(data))
            .await?;
        if has_outboard {
            self.storage
                .put(&Self::outboard_key(&hash_str), Bytes::from(outboard))
                .await?;
        }
        BlobMetadata::insert(&self.db, &hash_str, size as i64, has_outboard)?;
        Ok(hash)
    }

    pub async fn insert_partial(&self, hash: &Hash, size: u64) -> Result<()> {
        let hash_str = hash.to_string();
        let has_outboard = size > OUTBOARD_THRESHOLD as u64;
        BlobMetadata::insert_partial(&self.db, &hash_str, size as i64, has_outboard)?;
        Ok(())
    }

    pub(crate) async fn get_state(&self, hash: &Hash) -> Result<Option<BlobState>> {
        Ok(BlobMetadata::get_state(&self.db, &hash.to_string())?)
    }

    pub async fn list(&self) -> Result<Vec<Hash>> {
        let hash_strings = BlobMetadata::list_hashes(&self.db)?;
        let mut hashes = Vec::with_capacity(hash_strings.len());
        for s in hash_strings {
            match s.parse::<Hash>() {
                Ok(h) => hashes.push(h),
                Err(_) => {
                    warn!(hash = %s, "invalid hash in database, skipping");
                }
            }
        }
        Ok(hashes)
    }

    #[allow(dead_code)]
    pub async fn get_outboard(&self, hash: &Hash) -> Result<Option<Bytes>> {
        self.storage
            .get(&Self::outboard_key(&hash.to_string()))
            .await
    }

    #[allow(dead_code)]
    pub fn list_data_hashes_stream(&self) -> impl futures::Stream<Item = Result<String>> + '_ {
        self.storage.list_prefix("data/")
    }
}
