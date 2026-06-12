use std::path::Path;

use async_trait::async_trait;

use zim_core::linked_data::Link;

mod database;

use database::{Database, DatabaseError, LogEntry};
use zim_core::vault::{VaultId, VaultLog, VaultLogError};

#[derive(Debug, Clone)]
pub struct SqliteVaultLog {
    db: Database,
}

impl SqliteVaultLog {
    pub fn new(path: &Path) -> Result<Self, DatabaseError> {
        Ok(Self {
            db: Database::new(path)?,
        })
    }

    pub fn in_memory() -> Result<Self, DatabaseError> {
        Ok(Self {
            db: Database::in_memory()?,
        })
    }
}

#[async_trait]
impl VaultLog for SqliteVaultLog {
    type Error = DatabaseError;

    async fn exists(&self, id: VaultId) -> Result<bool, VaultLogError<Self::Error>> {
        LogEntry::exists(&self.db, id).map_err(VaultLogError::Provider)
    }

    async fn heads(
        &self,
        id: VaultId,
        height: u64,
    ) -> Result<Vec<Link>, VaultLogError<Self::Error>> {
        let strings = LogEntry::heads(&self.db, id, height).map_err(VaultLogError::Provider)?;
        let mut links = Vec::with_capacity(strings.len());
        for s in strings {
            let hash = s.parse().map_err(|_| {
                VaultLogError::Provider(DatabaseError::Deserialize(anyhow::anyhow!(
                    "invalid hash in log: {s}"
                )))
            })?;
            links.push(Link::new(zim_core::linked_data::LD_RAW_CODEC, hash));
        }
        Ok(links)
    }

    async fn append(
        &self,
        id: VaultId,
        name: String,
        current: Link,
        previous: Option<Link>,
        height: u64,
    ) -> Result<(), VaultLogError<Self::Error>> {
        let current_str = current.hash().to_string();
        let previous_str = previous.map(|l| l.hash().to_string());
        LogEntry::append(
            &self.db,
            id,
            &name,
            &current_str,
            previous_str.as_deref(),
            height,
        )
        .map_err(VaultLogError::Provider)
    }

    async fn height(&self, id: VaultId) -> Result<u64, VaultLogError<Self::Error>> {
        LogEntry::height(&self.db, id)
            .map_err(VaultLogError::Provider)?
            .ok_or(VaultLogError::HeadNotFound(0))
    }

    async fn has(&self, id: VaultId, link: Link) -> Result<Vec<u64>, VaultLogError<Self::Error>> {
        let hash_str = link.hash().to_string();
        LogEntry::has(&self.db, id, &hash_str).map_err(VaultLogError::Provider)
    }

    async fn list_vaults(&self) -> Result<Vec<VaultId>, VaultLogError<Self::Error>> {
        LogEntry::list_vault_ids(&self.db).map_err(VaultLogError::Provider)
    }
}

#[cfg(test)]
mod tests {
    fn test_vault_id(byte: u8) -> zim_core::vault::VaultId {
        zim_core::vault::VaultId::from_hash(zim_core::linked_data::Hash::new([byte; 32]))
    }

    use super::*;

    #[tokio::test]
    async fn test_sqlite_vault_log() {
        let log = SqliteVaultLog::in_memory().unwrap();
        let id = test_vault_id(1);

        assert!(!log.exists(id).await.unwrap());

        let link0 = Link::default();
        log.append(id, "test".into(), link0.clone(), None, 0)
            .await
            .unwrap();

        assert!(log.exists(id).await.unwrap());
        assert_eq!(log.height(id).await.unwrap(), 0);

        let head = log.head(id, None).await.unwrap();
        assert_eq!(head.height, 0);

        let vaults = log.list_vaults().await.unwrap();
        assert!(vaults.contains(&id));
    }
}
