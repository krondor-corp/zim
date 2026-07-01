use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use zim_core::linked_data::Link;

use zim_core::vault::{VaultId, VaultLog, VaultLogError};

/// In-memory vault log for tests.
#[derive(Debug, Clone, Default)]
pub struct MemoryVaultLog {
    inner: Arc<Mutex<HashMap<VaultId, Vec<Entry>>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Entry {
    name: String,
    current: Link,
    previous: Option<Link>,
    height: u64,
}

impl MemoryVaultLog {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("memory vault log error")]
pub struct MemoryVaultLogError;

#[async_trait]
impl VaultLog for MemoryVaultLog {
    type Error = MemoryVaultLogError;

    async fn exists(&self, id: VaultId) -> Result<bool, VaultLogError<Self::Error>> {
        let inner = self.inner.lock();
        Ok(inner.contains_key(&id))
    }

    async fn heads(
        &self,
        id: VaultId,
        height: u64,
    ) -> Result<Vec<Link>, VaultLogError<Self::Error>> {
        let inner = self.inner.lock();
        let entries = inner.get(&id).cloned().unwrap_or_default();
        Ok(entries
            .iter()
            .filter(|e| e.height == height)
            .map(|e| e.current.clone())
            .collect())
    }

    async fn append(
        &self,
        id: VaultId,
        name: String,
        current: Link,
        previous: Option<Link>,
        height: u64,
    ) -> Result<(), VaultLogError<Self::Error>> {
        let mut inner = self.inner.lock();
        let entries = inner.entry(id).or_default();

        if entries
            .iter()
            .any(|e| e.height == height && e.current == current)
        {
            return Err(VaultLogError::Conflict);
        }

        entries.push(Entry {
            name,
            current,
            previous,
            height,
        });
        Ok(())
    }

    async fn height(&self, id: VaultId) -> Result<u64, VaultLogError<Self::Error>> {
        let inner = self.inner.lock();
        inner
            .get(&id)
            .and_then(|entries| entries.iter().map(|e| e.height).max())
            .ok_or(VaultLogError::HeadNotFound(0))
    }

    async fn has(&self, id: VaultId, link: Link) -> Result<Vec<u64>, VaultLogError<Self::Error>> {
        let inner = self.inner.lock();
        let entries = inner.get(&id).cloned().unwrap_or_default();
        Ok(entries
            .iter()
            .filter(|e| e.current == link)
            .map(|e| e.height)
            .collect())
    }

    async fn list_vaults(&self) -> Result<Vec<VaultId>, VaultLogError<Self::Error>> {
        let inner = self.inner.lock();
        Ok(inner.keys().copied().collect())
    }
}

#[cfg(test)]
mod tests {
    fn test_vault_id(byte: u8) -> zim_core::vault::VaultId {
        zim_core::vault::VaultId::from_hash(zim_core::linked_data::Hash::new(&[byte; 32]))
    }

    use super::*;
    use zim_core::vault::Head;

    #[tokio::test]
    async fn test_memory_vault_log() {
        let log = MemoryVaultLog::new();
        let id = test_vault_id(1);

        assert!(!log.exists(id).await.unwrap());

        let link0 = Link::default();
        log.append(id, "test".into(), link0.clone(), None, 0)
            .await
            .unwrap();

        assert!(log.exists(id).await.unwrap());
        assert_eq!(log.height(id).await.unwrap(), 0);

        let heads = log.heads(id, 0).await.unwrap();
        assert_eq!(heads.len(), 1);

        let vaults = log.list_vaults().await.unwrap();
        assert!(vaults.contains(&id));
    }

    fn test_link(byte: u8) -> Link {
        let hash = zim_core::linked_data::Hash::new(&[byte; 32]);
        Link::new(zim_core::linked_data::LD_RAW_CODEC, hash)
    }

    #[tokio::test]
    async fn exponential_sample_for_chain_of_5() {
        let log = MemoryVaultLog::new();
        let id = test_vault_id(2);
        let links: Vec<Link> = (0..6).map(test_link).collect();
        let mut prev: Option<Link> = None;
        for (h, link) in links.iter().enumerate() {
            log.append(id, "v".into(), link.clone(), prev, h as u64)
                .await
                .unwrap();
            prev = Some(link.clone());
        }

        let sample = log.exponential_sample(id).await.unwrap();
        let heights: Vec<u64> = sample.iter().map(|h| h.height).collect();
        assert_eq!(heights, vec![5, 4, 3, 1, 0]);
        for head in sample {
            assert_eq!(head.link, links[head.height as usize]);
        }
    }

    #[tokio::test]
    async fn exponential_sample_for_unknown_vault_is_empty() {
        let log = MemoryVaultLog::new();
        let sample = log.exponential_sample(test_vault_id(3)).await.unwrap();
        assert!(sample.is_empty());
    }

    #[tokio::test]
    async fn probe_returns_deepest_match() {
        let log = MemoryVaultLog::new();
        let id = test_vault_id(4);
        for (h, link) in (0..4).map(test_link).enumerate() {
            let prev = if h == 0 {
                None
            } else {
                Some(test_link((h - 1) as u8))
            };
            log.append(id, "v".into(), link.clone(), prev, h as u64)
                .await
                .unwrap();
        }

        // Sample includes a far-future link we don't have plus two we do.
        let sample = vec![
            Head::new(test_link(99), 99), // we don't have this
            Head::new(test_link(3), 3),   // we have this — should win
            Head::new(test_link(1), 1),   // we have this too
        ];

        let result = log.probe(id, &sample).await.unwrap();
        assert_eq!(result, Some(Head::new(test_link(3), 3)));
    }

    #[tokio::test]
    async fn probe_returns_none_when_no_match() {
        let log = MemoryVaultLog::new();
        let id = test_vault_id(5);
        log.append(id, "v".into(), test_link(0), None, 0)
            .await
            .unwrap();

        let sample = vec![Head::new(test_link(99), 99), Head::new(test_link(42), 42)];
        let result = log.probe(id, &sample).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_conflict() {
        let log = MemoryVaultLog::new();
        let id = test_vault_id(6);
        let link = Link::default();

        log.append(id, "v".into(), link.clone(), None, 0)
            .await
            .unwrap();

        let result = log.append(id, "v".into(), link.clone(), None, 0).await;
        assert!(matches!(result, Err(VaultLogError::Conflict)));
    }
}
