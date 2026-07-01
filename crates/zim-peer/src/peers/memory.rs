//! In-memory [`PeerStore`] for tests.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use zim_did::Identity;

use zim_core::peers::{PeerEntry, PeerStore, PeerStoreError};

#[derive(Debug, Clone, Default)]
pub struct MemoryPeerStore {
    inner: Arc<RwLock<BTreeMap<String, PeerEntry>>>,
}

impl MemoryPeerStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryPeerError {}

#[async_trait]
impl PeerStore for MemoryPeerStore {
    type Error = MemoryPeerError;

    async fn list(&self) -> Result<Vec<PeerEntry>, PeerStoreError<Self::Error>> {
        let guard = self.inner.read().await;
        Ok(guard.values().cloned().collect())
    }

    async fn get(&self, nick: &str) -> Result<Option<PeerEntry>, PeerStoreError<Self::Error>> {
        let guard = self.inner.read().await;
        Ok(guard.get(nick).cloned())
    }

    async fn upsert(
        &self,
        nick: &str,
        identity: Identity,
        trusted: bool,
        notes: Option<String>,
    ) -> Result<(), PeerStoreError<Self::Error>> {
        let mut guard = self.inner.write().await;
        let prev = guard.get(nick);
        let added_at = prev.map(|p| p.added_at).unwrap_or_else(now_ts);
        let notes = notes.or_else(|| prev.and_then(|p| p.notes.clone()));
        guard.insert(
            nick.to_string(),
            PeerEntry {
                nick: nick.to_string(),
                identity,
                // The in-memory store (tests) only models direct contacts.
                via: None,
                trusted,
                added_at,
                notes,
            },
        );
        Ok(())
    }

    async fn remove(&self, nick: &str) -> Result<PeerEntry, PeerStoreError<Self::Error>> {
        let mut guard = self.inner.write().await;
        guard
            .remove(nick)
            .ok_or_else(|| PeerStoreError::NotFound(nick.to_string()))
    }
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}
