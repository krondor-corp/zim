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

    /// The record registered at `mountpoint`, if any.
    pub fn get(&self, mountpoint: &Path) -> Option<MountRecord> {
        self.load().into_iter().find(|r| r.mountpoint == mountpoint)
    }

    /// Register a new record. Returns `false` (and writes nothing) if its
    /// mountpoint is already taken — replacing goes through `remove` first.
    pub fn insert(&self, record: MountRecord) -> io::Result<bool> {
        let mut all = self.load();
        if all.iter().any(|r| r.mountpoint == record.mountpoint) {
            return Ok(false);
        }
        all.push(record);
        self.save_all(&all)?;
        Ok(true)
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

#[cfg(test)]
mod tests {
    use super::*;
    use zim_core::linked_data::Hash;

    fn vault(seed: u8) -> VaultId {
        VaultId::from_hash(Hash::new(&[seed; 32]))
    }

    fn record(vault_id: VaultId, mountpoint: &str, auto_mount: bool) -> MountRecord {
        MountRecord {
            vault_id,
            mountpoint: PathBuf::from(mountpoint),
            auto_mount,
            read_only: false,
        }
    }

    #[test]
    fn alice_cannot_register_a_second_mount_over_her_notes() {
        let home = tempfile::tempdir().unwrap();
        let store = MountStore::new(home.path());
        let notes = vault(1);
        let photos = vault(2);

        // Alice mounts her notes with auto-mount on.
        assert!(store.insert(record(notes, "/mnt/notes", true)).unwrap());

        // Re-adding the same path — even with another vault — is refused...
        assert!(!store.insert(record(notes, "/mnt/notes", false)).unwrap());
        assert!(!store.insert(record(photos, "/mnt/notes", false)).unwrap());

        // ...and the original registration (flags included) is untouched.
        let kept = store.get(Path::new("/mnt/notes")).unwrap();
        assert_eq!(kept.vault_id, notes);
        assert!(kept.auto_mount);
        assert_eq!(store.load().len(), 1);
    }

    #[test]
    fn alice_replaces_a_mount_by_removing_then_adding() {
        let home = tempfile::tempdir().unwrap();
        let store = MountStore::new(home.path());
        let notes = vault(1);
        let photos = vault(2);
        store.insert(record(notes, "/mnt/shared", false)).unwrap();

        // Remove frees the path for a fresh registration.
        assert!(store.remove(Path::new("/mnt/shared")).unwrap());
        assert!(store.insert(record(photos, "/mnt/shared", false)).unwrap());

        assert_eq!(
            store.get(Path::new("/mnt/shared")).unwrap().vault_id,
            photos
        );
    }
}
