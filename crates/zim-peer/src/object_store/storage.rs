use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use object_store::aws::AmazonS3Builder;
use object_store::local::LocalFileSystem;
use object_store::memory::InMemory;
use object_store::path::Path as ObjectPath;
use object_store::ObjectStore;
use serde::{Deserialize, Serialize};

use super::error::{BlobStoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StorageConfig {
    #[default]
    Memory,
    Local {
        path: PathBuf,
    },
    S3 {
        endpoint: String,
        access_key: String,
        secret_key: String,
        bucket: String,
        region: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Storage {
    inner: Arc<dyn ObjectStore>,
}

impl Storage {
    pub async fn new(config: StorageConfig) -> Result<Self> {
        let inner: Arc<dyn ObjectStore> = match &config {
            StorageConfig::Memory => Arc::new(InMemory::new()),
            StorageConfig::Local { path } => {
                tokio::fs::create_dir_all(path).await?;
                Arc::new(
                    LocalFileSystem::new_with_prefix(path)
                        .map_err(|e| BlobStoreError::InvalidConfig(e.to_string()))?,
                )
            }
            StorageConfig::S3 {
                endpoint,
                access_key,
                secret_key,
                bucket,
                region,
            } => {
                let store: Arc<dyn ObjectStore> = Arc::new(
                    AmazonS3Builder::new()
                        .with_endpoint(endpoint)
                        .with_access_key_id(access_key)
                        .with_secret_access_key(secret_key)
                        .with_bucket_name(bucket)
                        .with_region(region.as_deref().unwrap_or("us-east-1"))
                        .with_allow_http(endpoint.starts_with("http://"))
                        .build()
                        .map_err(|e| BlobStoreError::InvalidConfig(e.to_string()))?,
                );
                {
                    use futures::TryStreamExt;
                    let mut stream = store.list(Some(&ObjectPath::from("")));
                    match stream.try_next().await {
                        Ok(_) => {}
                        Err(object_store::Error::NotFound { .. }) => {
                            return Err(BlobStoreError::BucketNotFound(bucket.clone()));
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            if msg.contains("NoSuchBucket") {
                                return Err(BlobStoreError::BucketNotFound(bucket.clone()));
                            }
                            return Err(e.into());
                        }
                    }
                }
                store
            }
        };
        Ok(Self { inner })
    }

    pub fn memory() -> Self {
        Self {
            inner: Arc::new(InMemory::new()),
        }
    }

    pub async fn put(&self, key: &str, data: Bytes) -> Result<()> {
        self.inner.put(&ObjectPath::from(key), data.into()).await?;
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Option<Bytes>> {
        match self.inner.get(&ObjectPath::from(key)).await {
            Ok(result) => Ok(Some(result.bytes().await?)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        match self.inner.delete(&ObjectPath::from(key)).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_prefix(&self, prefix: &str) -> impl futures::Stream<Item = Result<String>> + '_ {
        use futures::StreamExt;
        let prefix = ObjectPath::from(prefix);
        let strip = prefix.as_ref().to_string();
        self.inner.list(Some(&prefix)).filter_map(move |r| {
            let strip = strip.clone();
            async move {
                match r {
                    Ok(meta) => meta
                        .location
                        .as_ref()
                        .strip_prefix(&strip)
                        .map(|s| Ok(s.to_string())),
                    Err(e) => Some(Err(e.into())),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_put_get_delete() {
        let s = Storage::memory();
        s.put("foo", Bytes::from("bar")).await.unwrap();
        assert_eq!(s.get("foo").await.unwrap().unwrap(), Bytes::from("bar"));
        s.delete("foo").await.unwrap();
        assert!(s.get("foo").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_local() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Storage::new(StorageConfig::Local {
            path: tmp.path().to_path_buf(),
        })
        .await
        .unwrap();
        s.put("k", Bytes::from("v")).await.unwrap();
        assert_eq!(s.get("k").await.unwrap().unwrap(), Bytes::from("v"));
    }
}
