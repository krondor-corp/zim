//! Mount lifecycle + the single-writer entry point.
//!
//! Owns the **long-lived vault handles** and their FUSE sessions. Each mount
//! opens its vault exactly once into an `Arc<RwLock<Vault>>` and hands it to
//! [`crate::fuse::FuseFs`], which serializes every mutation through the write lock
//! and stages it; a shared [`PendingCommit`] `save()`s once the mount goes quiet
//! — a single in-process writer advancing the chain linearly, one version per
//! editor save rather than per syscall.
//! This is deliberately *unlike* the daemon's HTTP path (which re-opens per
//! request and can fork the head under concurrency); see the FUSE plan.
//!
//! To keep that single-writer guarantee across *both* surfaces, the daemon's
//! HTTP vault-mutation handlers call [`MountManager::fs_add`] (etc.) first: if
//! the target vault is mounted they go through the mount's handle + shared
//! cache, so a `zim vault … add` to a mounted vault can't fork the head and is
//! immediately visible through the mountpoint. Their `save()` sweeps up any
//! staged mount work, so they mark the pending commit done.
//!
//! Remote sync is reconciled by a per-mount watcher: when an incoming
//! `HeadAdvanced` → `PullFromPeer` advances the local log past the mount's
//! pinned in-memory manifest, the watcher commits any staged mount work, then
//! reloads the handle to the canonical head and drops the stale cache. (Concurrent local-mount *and* remote writes
//! to the same vault still pick the canonical fork — full merge is future
//! work.)

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::fuse::inode_table::InodeTable;
use crate::fuse::{
    spawn_mount, BackgroundSession, FileCache, FileCacheConfig, FuseFs, OnCommit, PendingCommit,
};
use tokio::runtime::Handle;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use zim_core::fs::AbsPath;
use zim_core::linked_data::Link;
use zim_core::vault::{Vault, VaultId};
use zim_peer::BlobsProvider;
use zim_peer::{Peer, SqliteVaultLog};

use super::store::{MountRecord, MountStore};

type DaemonPeer = Peer<SqliteVaultLog>;
type DaemonVault = Vault<BlobsProvider, SqliteVaultLog>;
type DaemonPending = PendingCommit<BlobsProvider, SqliteVaultLog>;

/// A running mount: the FUSE session (dropping it unmounts) plus the shared
/// handle + cache the daemon reuses to keep HTTP writes coherent, and a
/// watcher that reloads the handle when remote sync advances the log.
struct LiveMount {
    /// Dropping this unmounts the filesystem and ends its session thread.
    _session: BackgroundSession,
    vault_id: VaultId,
    vault: Arc<RwLock<DaemonVault>>,
    cache: FileCache,
    /// Mount mutations staged on `vault` but not yet committed.
    pending: Arc<DaemonPending>,
    /// Background reconciliation task; aborted when the mount goes away.
    watcher: JoinHandle<()>,
}

impl Drop for LiveMount {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

/// How often the watcher checks whether the log head moved past the mount's
/// in-memory manifest (i.e. a remote peer advanced it).
const RECONCILE_POLL: Duration = Duration::from_secs(3);

/// The new head after a routed mutation.
pub struct Committed {
    pub link: Link,
    pub height: u64,
}

/// Owns mount registrations + the live sessions.
pub struct MountManager {
    peer: DaemonPeer,
    rt: Handle,
    store: MountStore,
    live: Mutex<HashMap<PathBuf, LiveMount>>,
}

/// A row for `mount list`: the persisted record plus whether it's live.
#[derive(Debug, Clone)]
pub struct MountStatus {
    pub vault_id: VaultId,
    pub mountpoint: PathBuf,
    pub read_only: bool,
    pub auto_mount: bool,
    pub mounted: bool,
}

fn abs(path: &str) -> AbsPath {
    AbsPath::from_abs(PathBuf::from(path))
}

/// Best-effort clear of a stale FUSE mount at `path`. A daemon that was hard-
/// killed (`kill -9`) can't run `Drop`, and macFUSE's AutoUnmount doesn't
/// reliably fire on hard kill — leaving a dead "Device not configured" mount
/// that blocks the next mount with EAGAIN. Force-unmount it first; a no-op
/// (ignored error) when nothing is mounted there.
fn force_unmount_stale(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("/sbin/umount")
        .arg("-f")
        .arg(path)
        .output();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("fusermount")
        .args(["-u", "-z"])
        .arg(path)
        .output();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let _ = path;
}

/// Commit a mount's staged work, then drop it (dropping the session
/// unmounts). Without the commit, the debounce timer would still fire a
/// quiet window later on this now-orphaned handle — after HTTP writes have
/// stopped routing through it, so a write in between chains off the same
/// parent and forks the head. On shutdown the timer would instead die with
/// the runtime, losing the work.
async fn unmount(mountpoint: &Path, mount: LiveMount) {
    if let Err(e) = mount.pending.flush().await {
        tracing::warn!("commit {} before unmount: {e}", mountpoint.display());
    }
    drop(mount);
}

impl MountManager {
    pub fn new(peer: DaemonPeer, rt: Handle, home: &Path) -> Self {
        Self {
            peer,
            rt,
            store: MountStore::new(home),
            live: Mutex::new(HashMap::new()),
        }
    }

    /// Open the vault, build the FUSE filesystem over a long-lived handle +
    /// shared cache, and spawn the mount session.
    pub async fn start(&self, record: MountRecord) -> anyhow::Result<()> {
        let mountpoint = record.mountpoint.clone();
        if self.live.lock().unwrap().contains_key(&mountpoint) {
            anyhow::bail!("already mounted at {}", mountpoint.display());
        }

        let vault = self
            .peer
            .vault(record.vault_id)
            .await
            .map_err(|e| anyhow::anyhow!("open vault {}: {e}", record.vault_id))?;
        let vault = Arc::new(RwLock::new(vault));
        let cache = FileCache::new(FileCacheConfig::default());

        // After each (debounced) mount commit, announce the new head to peers.
        // Fired from inside the runtime under the write guard → spawn, and
        // the announce reads the head once the guard is released.
        let on_commit: OnCommit = {
            let peer = self.peer.clone();
            let rt = self.rt.clone();
            let vault = vault.clone();
            Arc::new(move || {
                let (peer, vault) = (peer.clone(), vault.clone());
                rt.spawn(async move {
                    let guard = vault.read().await;
                    if let Ok(head) = guard.head().await {
                        peer.announce_head(&guard, head).await;
                    }
                });
            })
        };
        let pending = Arc::new(PendingCommit::new(
            self.rt.clone(),
            vault.clone(),
            Some(on_commit),
        ));

        let fs = FuseFs::new(
            self.rt.clone(),
            vault.clone(),
            cache.clone(),
            record.read_only,
            pending.clone(),
        );
        let volname = mountpoint
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "zim".to_string());
        // Stable per-vault volume identity (see spawn_mount): first 3 bytes
        // of the vault id, clamped away from macFUSE's reserved 0/0xFFFFFF.
        let fsid = {
            let b = record.vault_id.as_bytes();
            let raw = u32::from_be_bytes([0, b[0], b[1], b[2]]);
            raw.clamp(1, 0xFF_FFFE)
        };
        // Clear any dead mount a previous hard-killed daemon left here, or the
        // fresh mount fails with EAGAIN ("Resource temporarily unavailable").
        force_unmount_stale(&mountpoint);
        let session = spawn_mount(fs, &mountpoint, record.read_only, &volname, fsid)
            .map_err(|e| anyhow::anyhow!("mount {}: {e}", mountpoint.display()))?;

        // Reconcile with remote sync: the long-lived handle's in-memory tree
        // (`manifest_link`) is pinned, but `head()` reflects the live log. When
        // a remote peer advances the log past our manifest, reload the handle to
        // the canonical head and drop the stale cache. A reload discards staged
        // mutations, so commit them first — all under one write guard, so the
        // mount can't stage anything between the commit and the swap.
        let watcher = {
            let peer = self.peer.clone();
            let vault = vault.clone();
            let cache = cache.clone();
            let pending = pending.clone();
            let id = record.vault_id;
            self.rt.spawn(async move {
                loop {
                    tokio::time::sleep(RECONCILE_POLL).await;
                    let (latest, current) = {
                        let g = vault.read().await;
                        (
                            g.head().await.ok().map(|h| h.link),
                            g.manifest_link().clone(),
                        )
                    };
                    let Some(latest) = latest else { continue };
                    // By hash, not `Link`: the codec isn't part of identity.
                    if latest.hash() == current.hash() {
                        continue; // our tree is the canonical head
                    }
                    let mut g = vault.write().await;
                    if let Err(e) = pending.flush_locked(&mut g).await {
                        tracing::warn!(%id, "mount commit before reload failed: {e}");
                        continue; // keep the staged work; retry next poll
                    }
                    match peer.vault(id).await {
                        Ok(fresh) => {
                            *g = fresh;
                            cache.invalidate_all();
                            tracing::info!(%id, "mount reloaded to advanced head");
                        }
                        Err(e) => tracing::warn!(%id, "mount reload failed: {e}"),
                    }
                }
            })
        };

        self.live.lock().unwrap().insert(
            mountpoint,
            LiveMount {
                _session: session,
                vault_id: record.vault_id,
                vault,
                cache,
                pending,
                watcher,
            },
        );
        Ok(())
    }

    /// Persist a new mount registration and start it.
    pub async fn add(
        &self,
        vault_id: VaultId,
        mountpoint: PathBuf,
        auto_mount: bool,
        read_only: bool,
    ) -> anyhow::Result<()> {
        let record = MountRecord {
            vault_id,
            mountpoint,
            auto_mount,
            read_only,
        };
        self.store.upsert(record.clone())?;
        self.start(record).await
    }

    /// Unmount every live mount (registrations kept). Called on daemon
    /// shutdown so mounts spin down with the process instead of lingering
    /// as dead volumes; `start_auto` on the next boot brings the `--auto`
    /// ones back.
    pub async fn stop_all(&self) {
        let drained: Vec<_> = self.live.lock().unwrap().drain().collect();
        for (mountpoint, mount) in drained {
            tracing::info!("unmounting {} (daemon shutdown)", mountpoint.display());
            unmount(&mountpoint, mount).await;
        }
    }

    /// Unmount a registered mount, keeping its registration. Idempotent:
    /// stopping an already-stopped mount succeeds.
    ///
    /// A live mount commits its staged work, then unmounts. A registration
    /// that isn't live may still have a **ghost** in the OS mount table — a
    /// hard-killed daemon never ran `Drop` — so it gets a best-effort
    /// force-unmount. Errors only on an unknown mountpoint.
    pub async fn stop(&self, mountpoint: &Path) -> anyhow::Result<()> {
        let mount = self.live.lock().unwrap().remove(mountpoint);
        if let Some(mount) = mount {
            unmount(mountpoint, mount).await;
            return Ok(());
        }
        if !self.store.load().iter().any(|r| r.mountpoint == mountpoint) {
            anyhow::bail!("no mount registered at {}", mountpoint.display());
        }
        force_unmount_stale(mountpoint);
        Ok(())
    }

    /// Update a registration's `auto_mount` / `read_only` flags in
    /// place. `read_only` changes on a live mount trigger a remount
    /// (the FUSE session is created with the flag baked in).
    pub async fn set(
        &self,
        mountpoint: &Path,
        auto_mount: Option<bool>,
        read_only: Option<bool>,
    ) -> anyhow::Result<MountStatus> {
        let mut record = self
            .store
            .load()
            .into_iter()
            .find(|r| r.mountpoint == mountpoint)
            .ok_or_else(|| anyhow::anyhow!("no mount registered at {}", mountpoint.display()))?;
        if let Some(auto) = auto_mount {
            record.auto_mount = auto;
        }
        let ro_changed = matches!(read_only, Some(ro) if ro != record.read_only);
        if let Some(ro) = read_only {
            record.read_only = ro;
        }
        self.store.upsert(record.clone())?;
        let was_live = self.live.lock().unwrap().contains_key(mountpoint);
        if ro_changed && was_live {
            self.stop(mountpoint).await?;
            self.start(record.clone()).await?;
        }
        Ok(MountStatus {
            vault_id: record.vault_id,
            mountpoint: record.mountpoint,
            read_only: record.read_only,
            auto_mount: record.auto_mount,
            mounted: was_live,
        })
    }

    /// Unmount (if live) and drop the registration.
    pub async fn remove(&self, mountpoint: &Path) -> anyhow::Result<bool> {
        let mount = self.live.lock().unwrap().remove(mountpoint);
        if let Some(mount) = mount {
            unmount(mountpoint, mount).await;
        }
        Ok(self.store.remove(mountpoint)?)
    }

    /// Every registration with its live status.
    pub fn list(&self) -> Vec<MountStatus> {
        let live = self.live.lock().unwrap();
        self.store
            .load()
            .into_iter()
            .map(|r| MountStatus {
                vault_id: r.vault_id,
                mounted: live.contains_key(&r.mountpoint),
                mountpoint: r.mountpoint,
                read_only: r.read_only,
                auto_mount: r.auto_mount,
            })
            .collect()
    }

    /// Start every `auto_mount` registration. Failures are logged, not fatal —
    /// a missing mountpoint or busy vault shouldn't take down daemon boot.
    pub async fn start_auto(&self) {
        for record in self.store.load().into_iter().filter(|r| r.auto_mount) {
            let mountpoint = record.mountpoint.clone();
            if let Err(e) = self.start(record).await {
                tracing::warn!("auto-mount {} failed: {e}", mountpoint.display());
            }
        }
    }

    // -- routed mutations (HTTP-API coherence) -------------------------------
    //
    // Each returns `None` when `id` isn't mounted (the caller falls back to its
    // own freshly-opened handle); otherwise it locks the mount's writer, stages
    // the op, saves, announces the new head, and invalidates the shared cache so
    // the mount reflects the change immediately.

    /// The live handle, cache + pending commit for a mounted `vault_id`.
    fn live_of(
        &self,
        id: VaultId,
    ) -> Option<(Arc<RwLock<DaemonVault>>, FileCache, Arc<DaemonPending>)> {
        let live = self.live.lock().unwrap();
        live.values()
            .find(|m| m.vault_id == id)
            .map(|m| (m.vault.clone(), m.cache.clone(), m.pending.clone()))
    }

    /// Save under the held write guard, announce the new head, return it.
    /// The save sweeps up any staged mount work, so the pending commit has
    /// nothing left to do.
    async fn save_announce(
        &self,
        guard: &mut DaemonVault,
        pending: &DaemonPending,
    ) -> anyhow::Result<Committed> {
        let link = guard.save().await?;
        pending.mark_committed();
        let head = guard.head().await?;
        self.peer.announce_head(guard, head.clone()).await;
        Ok(Committed {
            link,
            height: head.height,
        })
    }

    /// Route a file write through the mount (if mounted).
    pub async fn fs_add(
        &self,
        id: VaultId,
        path: String,
        bytes: Vec<u8>,
    ) -> Option<anyhow::Result<Committed>> {
        let (vault, cache, pending) = self.live_of(id)?;
        let result = async {
            let mut g = vault.write().await;
            g.fs().add(&abs(&path), Cursor::new(bytes)).await?;
            self.save_announce(&mut g, &pending).await
        }
        .await;
        cache.invalidate(&path);
        cache.invalidate(&InodeTable::parent_path(&path));
        Some(result)
    }

    /// Route a mkdir through the mount (if mounted).
    pub async fn fs_mkdir(&self, id: VaultId, path: String) -> Option<anyhow::Result<Committed>> {
        let (vault, cache, pending) = self.live_of(id)?;
        let result = async {
            let mut g = vault.write().await;
            g.fs().mkdir(&abs(&path), false).await?;
            self.save_announce(&mut g, &pending).await
        }
        .await;
        cache.invalidate(&path);
        cache.invalidate(&InodeTable::parent_path(&path));
        Some(result)
    }

    /// Route a remove through the mount (if mounted).
    pub async fn fs_rm(&self, id: VaultId, path: String) -> Option<anyhow::Result<Committed>> {
        let (vault, cache, pending) = self.live_of(id)?;
        let result = async {
            let mut g = vault.write().await;
            g.fs().rm(&abs(&path)).await?;
            self.save_announce(&mut g, &pending).await
        }
        .await;
        cache.invalidate(&path);
        cache.invalidate(&InodeTable::parent_path(&path));
        Some(result)
    }

    /// Route a move through the mount (if mounted).
    pub async fn fs_mv(
        &self,
        id: VaultId,
        from: String,
        to: String,
    ) -> Option<anyhow::Result<Committed>> {
        let (vault, cache, pending) = self.live_of(id)?;
        let result = async {
            let mut g = vault.write().await;
            g.fs().mv(&abs(&from), &abs(&to)).await?;
            self.save_announce(&mut g, &pending).await
        }
        .await;
        for p in [&from, &to] {
            cache.invalidate(p);
            cache.invalidate(&InodeTable::parent_path(p));
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use zim_core::linked_data::Hash;
    use zim_crypto::PrivateKey;

    use super::*;

    /// Alice's daemon: a mount manager over an offline peer, rooted at `home`.
    async fn alice_daemon(home: &Path) -> MountManager {
        let peer = Peer::builder()
            .with_secret(PrivateKey::generate())
            .with_log(SqliteVaultLog::in_memory().unwrap())
            .with_blobs(BlobsProvider::memory().await.unwrap())
            .build()
            .await
            .unwrap();
        MountManager::new(peer, Handle::current(), home)
    }

    #[tokio::test]
    async fn stopping_a_stopped_mount_is_a_no_op() {
        let home = tempfile::tempdir().unwrap();
        let alice = alice_daemon(home.path()).await;

        // Alice registered `notes` in a previous daemon life; this daemon
        // never started it, so it reads `stopped`.
        let notes = home.path().join("notes");
        alice
            .store
            .upsert(MountRecord {
                vault_id: VaultId::from_hash(Hash::new(&[7u8; 32])),
                mountpoint: notes.clone(),
                auto_mount: false,
                read_only: false,
            })
            .unwrap();
        assert!(!alice.list()[0].mounted);

        // `zim mount stop notes` succeeds — twice — and keeps the registration.
        alice.stop(&notes).await.unwrap();
        alice.stop(&notes).await.unwrap();
        assert_eq!(alice.list().len(), 1);
    }

    #[tokio::test]
    async fn stopping_an_unknown_mountpoint_errors() {
        let home = tempfile::tempdir().unwrap();
        let alice = alice_daemon(home.path()).await;

        let err = alice.stop(&home.path().join("nowhere")).await.unwrap_err();
        assert!(err.to_string().contains("no mount registered"));
    }
}
