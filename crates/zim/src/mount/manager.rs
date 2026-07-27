//! Mount lifecycle + the single-writer entry point.
//!
//! Owns the **long-lived vault handles** and their FUSE sessions. Each mount
//! opens its vault exactly once into an `Arc<RwLock<Vault>>` and hands it to
//! [`crate::fuse::FuseFs`], which serializes every mutation through the write lock
//! then `save()`s — a single in-process writer advancing the chain linearly.
//! This is deliberately *unlike* the daemon's HTTP path (which re-opens per
//! request and can fork the head under concurrency); see the FUSE plan.
//!
//! To keep that single-writer guarantee across *both* surfaces, the daemon's
//! HTTP vault-mutation handlers call [`MountManager::fs_add`] (etc.) first: if
//! the target vault is mounted they go through the mount's handle + shared
//! cache, so a `zim vault … add` to a mounted vault can't fork the head and is
//! immediately visible through the mountpoint.
//!
//! Remote sync is reconciled by a per-mount watcher: when an incoming
//! `HeadAdvanced` → `PullFromPeer` advances the local log past the mount's
//! pinned in-memory manifest, the watcher reloads the handle to the canonical
//! head and drops the stale cache. (Concurrent local-mount *and* remote writes
//! to the same vault still pick the canonical fork — full merge is future
//! work.)

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::fuse::inode_table::InodeTable;
use crate::fuse::{spawn_mount, BackgroundSession, FileCache, FileCacheConfig, FuseFs};
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

/// A running mount: the FUSE session (dropping it unmounts) plus the shared
/// handle + cache the daemon reuses to keep HTTP writes coherent, and a
/// watcher that reloads the handle when remote sync advances the log.
struct LiveMount {
    /// Dropping this unmounts the filesystem and ends its session thread.
    _session: BackgroundSession,
    vault_id: VaultId,
    vault: Arc<RwLock<DaemonVault>>,
    cache: FileCache,
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

        // After each committed FUSE mutation, announce the new head to peers.
        let on_commit: Arc<dyn Fn() + Send + Sync> = {
            let peer = self.peer.clone();
            let rt = self.rt.clone();
            let vault = vault.clone();
            Arc::new(move || {
                let (peer, rt, vault) = (peer.clone(), rt.clone(), vault.clone());
                rt.block_on(async move {
                    let guard = vault.read().await;
                    if let Ok(head) = guard.head().await {
                        peer.announce_head(&guard, head).await;
                    }
                });
            })
        };

        let fs = FuseFs::new(
            self.rt.clone(),
            vault.clone(),
            cache.clone(),
            record.read_only,
            Some(on_commit),
        );
        let volname = mountpoint
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "zim".to_string());
        // Clear any dead mount a previous hard-killed daemon left here, or the
        // fresh mount fails with EAGAIN ("Resource temporarily unavailable").
        force_unmount_stale(&mountpoint);
        let session = spawn_mount(fs, &mountpoint, record.read_only, &volname)
            .map_err(|e| anyhow::anyhow!("mount {}: {e}", mountpoint.display()))?;

        // Reconcile with remote sync: the long-lived handle's in-memory tree
        // (`manifest_link`) is pinned, but `head()` reflects the live log. When
        // a remote peer advances the log past our manifest, reload the handle to
        // the canonical head and drop the stale cache.
        let watcher = {
            let peer = self.peer.clone();
            let vault = vault.clone();
            let cache = cache.clone();
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
                    if latest == current {
                        continue; // our tree is the canonical head
                    }
                    match peer.vault(id).await {
                        Ok(fresh) => {
                            *vault.write().await = fresh;
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

    /// Unmount a live mount (keeps its registration).
    /// Unmount every live mount (registrations kept). Called on daemon
    /// shutdown so mounts spin down with the process instead of lingering
    /// as dead volumes; `start_auto` on the next boot brings the `--auto`
    /// ones back.
    pub fn stop_all(&self) {
        let mut live = self.live.lock().unwrap();
        for (mountpoint, _mount) in live.drain() {
            // Dropping the LiveMount drops its BackgroundSession → unmount.
            tracing::info!("unmounting {} (daemon shutdown)", mountpoint.display());
        }
    }

    pub fn stop(&self, mountpoint: &Path) -> anyhow::Result<()> {
        match self.live.lock().unwrap().remove(mountpoint) {
            Some(_) => Ok(()), // drop unmounts
            None => anyhow::bail!("not mounted: {}", mountpoint.display()),
        }
    }

    /// Unmount (if live) and drop the registration.
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
            self.stop(mountpoint)?;
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

    pub fn remove(&self, mountpoint: &Path) -> anyhow::Result<bool> {
        self.live.lock().unwrap().remove(mountpoint);
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

    /// The live handle + cache for a mounted `vault_id`, if any.
    fn live_of(&self, id: VaultId) -> Option<(Arc<RwLock<DaemonVault>>, FileCache)> {
        let live = self.live.lock().unwrap();
        live.values()
            .find(|m| m.vault_id == id)
            .map(|m| (m.vault.clone(), m.cache.clone()))
    }

    /// Save under the held write guard, announce the new head, return it.
    async fn save_announce(&self, guard: &mut DaemonVault) -> anyhow::Result<Committed> {
        let link = guard.save().await?;
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
        let (vault, cache) = self.live_of(id)?;
        let result = async {
            let mut g = vault.write().await;
            g.fs().add(&abs(&path), Cursor::new(bytes)).await?;
            self.save_announce(&mut g).await
        }
        .await;
        cache.invalidate(&path);
        cache.invalidate(&InodeTable::parent_path(&path));
        Some(result)
    }

    /// Route a mkdir through the mount (if mounted).
    pub async fn fs_mkdir(&self, id: VaultId, path: String) -> Option<anyhow::Result<Committed>> {
        let (vault, cache) = self.live_of(id)?;
        let result = async {
            let mut g = vault.write().await;
            g.fs().mkdir(&abs(&path), false).await?;
            self.save_announce(&mut g).await
        }
        .await;
        cache.invalidate(&path);
        cache.invalidate(&InodeTable::parent_path(&path));
        Some(result)
    }

    /// Route a remove through the mount (if mounted).
    pub async fn fs_rm(&self, id: VaultId, path: String) -> Option<anyhow::Result<Committed>> {
        let (vault, cache) = self.live_of(id)?;
        let result = async {
            let mut g = vault.write().await;
            g.fs().rm(&abs(&path)).await?;
            self.save_announce(&mut g).await
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
        let (vault, cache) = self.live_of(id)?;
        let result = async {
            let mut g = vault.write().await;
            g.fs().mv(&abs(&from), &abs(&to)).await?;
            self.save_announce(&mut g).await
        }
        .await;
        for p in [&from, &to] {
            cache.invalidate(p);
            cache.invalidate(&InodeTable::parent_path(p));
        }
        Some(result)
    }
}
