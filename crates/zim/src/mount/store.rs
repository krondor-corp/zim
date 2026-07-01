//! Mount registry persistence.
//!
//! Mounts are daemon-local state — vault id, where it's mounted, and a couple
//! of flags — so they live in a small JSON file at `$ZIM_HOME/state/mounts.json`
//! rather than the vault log. Keyed by `mountpoint`: you can't mount two
//! vaults at one path.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zim_core::vault::VaultId;

/// One persisted mount registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountRecord {
    pub vault_id: VaultId,
    pub mountpoint: PathBuf,
    #[serde(default)]
    pub auto_mount: bool,
    #[serde(default)]
    pub read_only: bool,
}

/// JSON-backed mount registry.
pub struct MountStore {
    path: PathBuf,
}

impl MountStore {
    pub fn new(home: &Path) -> Self {
        Self {
            path: crate::context::paths::state_dir(home).join("mounts.json"),
        }
    }

    /// All persisted records (empty if the file is missing or unreadable).
    pub fn load(&self) -> Vec<MountRecord> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save_all(&self, records: &[MountRecord]) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(records).map_err(io::Error::other)?;
        std::fs::write(&self.path, json)
    }

    /// Insert or replace the record for its mountpoint.
    pub fn upsert(&self, record: MountRecord) -> io::Result<()> {
        let mut all = self.load();
        all.retain(|r| r.mountpoint != record.mountpoint);
        all.push(record);
        self.save_all(&all)
    }

    /// Drop the record at `mountpoint`. Returns whether one was removed.
    pub fn remove(&self, mountpoint: &Path) -> io::Result<bool> {
        let mut all = self.load();
        let before = all.len();
        all.retain(|r| r.mountpoint != mountpoint);
        let removed = all.len() != before;
        self.save_all(&all)?;
        Ok(removed)
    }
}
