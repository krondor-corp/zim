//! `fuser::Filesystem` over a long-lived [`Vault`].
//!
//! Reads serve from the in-memory tree under a read lock (fast); mutations
//! take the write lock, apply the `fs()` op, and `save()` — committing a new
//! manifest head in-process, atomically under a single lock. There is no HTTP
//! self-round-trip (the old `_zim-peer` design persisted via the daemon's own
//! API); the `Vault` *is* the persistence layer.
//!
//! FUSE callbacks are synchronous, so async vault ops run via
//! `Handle::block_on`. The mount's session runs on its own (non-runtime)
//! thread, so blocking there is safe.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::future::Future;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty,
    ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr, Request, TimeOrNow,
};
use tokio::runtime::Handle;
use tokio::sync::RwLock;

use zim_core::blobs::BlobStore;
use zim_core::fs::{AbsPath, FsError};
use zim_core::vault::log::VaultLog;
use zim_core::vault::Vault;

use crate::cache::{CachedAttr, CachedContent, CachedDirEntry, FileCache};
use crate::inode_table::InodeTable;

/// Pending whole-file write buffer, keyed by file handle. We buffer the whole
/// file and replace it on flush — simple and correct for the common
/// `echo`/`cp`/editor-rewrite cases; large-file streaming is a later concern.
struct WriteBuffer {
    data: Vec<u8>,
    dirty: bool,
}

/// FUSE filesystem over a single vault.
pub struct FuseFs<B: BlobStore, L: VaultLog> {
    rt: Handle,
    /// The long-lived, single-writer handle on the mounted vault.
    vault: Arc<RwLock<Vault<B, L>>>,
    inodes: Mutex<InodeTable>,
    write_buffers: Mutex<HashMap<u64, WriteBuffer>>,
    cache: FileCache,
    read_only: bool,
    next_fh: AtomicU64,
    /// Invoked after every committed mutation (the daemon wires this to
    /// `Peer::announce_head`). `None` in tests / standalone mounts.
    on_commit: Option<Arc<dyn Fn() + Send + Sync>>,
}

const ATTR_TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u32 = 512;

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Map a vault FS error onto a POSIX errno.
fn errno(e: &FsError) -> libc::c_int {
    match e {
        FsError::PathNotFound(_) => libc::ENOENT,
        // Covers "already exists with incompatible kind", traversal through a
        // file, root mutation, self-containing move. EEXIST is the common
        // case (create/mkdir over an existing name); good enough for v1.
        FsError::CannotMutate(_, _) => libc::EEXIST,
        FsError::ShareNotFound => libc::EACCES,
        FsError::Backing(_) => libc::EIO,
    }
}

/// `Vault::save`'s error type isn't `FsError`; flatten it into one so it rides
/// the same errno mapping.
fn save_err<E: std::fmt::Display>(e: E) -> FsError {
    FsError::Backing(anyhow::anyhow!("vault save: {e}"))
}

fn abs(path: &str) -> AbsPath {
    AbsPath::from_abs(std::path::PathBuf::from(path))
}

fn build_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

impl<B: BlobStore, L: VaultLog> FuseFs<B, L> {
    /// `cache` is supplied by the caller (rather than built here) so the
    /// daemon can keep a clone and invalidate it when it mutates the same
    /// vault out-of-band — keeping the mount coherent with HTTP-API writes.
    pub fn new(
        rt: Handle,
        vault: Arc<RwLock<Vault<B, L>>>,
        cache: FileCache,
        read_only: bool,
        on_commit: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        Self {
            rt,
            vault,
            inodes: Mutex::new(InodeTable::new()),
            write_buffers: Mutex::new(HashMap::new()),
            cache,
            read_only,
            next_fh: AtomicU64::new(1),
            on_commit,
        }
    }

    fn block<F: Future>(&self, fut: F) -> F::Output {
        self.rt.block_on(fut)
    }

    fn next_handle(&self) -> u64 {
        self.next_fh.fetch_add(1, Ordering::SeqCst)
    }

    /// macOS resource-fork / metadata noise we hide from listings + lookups.
    fn should_filter(name: &str) -> bool {
        name.starts_with("._") || name == ".DS_Store" || name == ".Spotlight-V100"
    }

    fn path_of(&self, ino: u64) -> Option<String> {
        self.inodes
            .lock()
            .unwrap()
            .get_path(ino)
            .map(str::to_string)
    }

    fn make_attr(ino: u64, attr: &CachedAttr) -> FileAttr {
        let kind = if attr.is_dir {
            FileType::Directory
        } else {
            FileType::RegularFile
        };
        let mtime = UNIX_EPOCH + Duration::from_secs(attr.mtime.max(0) as u64);
        let perm = if attr.is_dir { 0o755 } else { 0o644 };
        FileAttr {
            ino,
            size: attr.size,
            blocks: attr.size.div_ceil(BLOCK_SIZE as u64),
            atime: mtime,
            mtime,
            ctime: mtime,
            crtime: mtime,
            kind,
            perm,
            nlink: 1,
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    /// Run a committed mutation: block on `fut` (which takes the write lock,
    /// stages the op, and `save()`s), then invalidate cache + fire `on_commit`.
    fn commit(
        &self,
        invalidate: &[&str],
        fut: impl Future<Output = Result<(), FsError>>,
    ) -> Result<(), libc::c_int> {
        match self.block(fut) {
            Ok(()) => {
                for p in invalidate {
                    self.cache.invalidate(p);
                }
                if let Some(cb) = &self.on_commit {
                    cb();
                }
                Ok(())
            }
            Err(e) => Err(errno(&e)),
        }
    }

    fn fetch_attr(&self, path: &str) -> Option<CachedAttr> {
        if let Some(attr) = self.cache.get_attr(path) {
            return Some(attr);
        }
        if self.cache.is_negative(path) {
            return None;
        }
        let vault = self.vault.clone();
        let path_str = path.to_string();
        let result = self.block(async move {
            if path_str == "/" {
                return Some(CachedAttr {
                    size: 0,
                    is_dir: true,
                    mime_type: None,
                    mtime: now_unix(),
                });
            }
            let parent = InodeTable::parent_path(&path_str);
            let filename = InodeTable::filename(&path_str);
            let guard = vault.read().await;
            let entries = guard.fs().ls(&abs(&parent)).await.ok()?;
            for (entry_path, entry) in entries {
                let name = entry_path.file_name()?.to_string_lossy().to_string();
                if name != filename {
                    continue;
                }
                let is_dir = entry.is_dir();
                let mime_type = entry.mime().map(|m| m.to_string());
                // No size lives in the manifest entry → decrypt to measure.
                // The attr cache keeps this off the hot path.
                let size = if is_dir {
                    0
                } else {
                    guard
                        .fs()
                        .cat(&abs(&path_str))
                        .await
                        .map(|d| d.len() as u64)
                        .unwrap_or(0)
                };
                return Some(CachedAttr {
                    size,
                    is_dir,
                    mime_type,
                    mtime: now_unix(),
                });
            }
            None
        });
        match &result {
            Some(attr) => self.cache.put_attr(path, attr.clone()),
            None => self.cache.put_negative(path),
        }
        result
    }

    fn fetch_dir(&self, path: &str) -> Option<Vec<CachedDirEntry>> {
        if let Some(entries) = self.cache.get_dir(path) {
            return Some(entries);
        }
        let vault = self.vault.clone();
        let path_str = path.to_string();
        let result = self.block(async move {
            let guard = vault.read().await;
            let map = guard.fs().ls(&abs(&path_str)).await.ok()?;
            let entries: Vec<CachedDirEntry> = map
                .into_iter()
                .filter_map(|(entry_path, entry)| {
                    let name = entry_path.file_name()?.to_string_lossy().to_string();
                    if Self::should_filter(&name) {
                        return None;
                    }
                    Some(CachedDirEntry {
                        name,
                        is_dir: entry.is_dir(),
                    })
                })
                .collect();
            Some(entries)
        });
        if let Some(ref entries) = result {
            self.cache.put_dir(path, entries.clone());
        }
        result
    }

    fn fetch_content(&self, path: &str) -> Option<CachedContent> {
        if let Some(content) = self.cache.get_content(path) {
            return Some(content);
        }
        let vault = self.vault.clone();
        let path_str = path.to_string();
        let result = self.block(async move {
            let guard = vault.read().await;
            guard
                .fs()
                .cat(&abs(&path_str))
                .await
                .ok()
                .map(|data| CachedContent {
                    data: Arc::new(data),
                    mime_type: "application/octet-stream".to_string(),
                })
        });
        if let Some(ref content) = result {
            self.cache.put_content(path, content.clone());
        }
        result
    }

    /// Stage `data` as the whole body of `path` and commit. Shared by
    /// create/write-flush/truncate.
    fn write_whole(&self, path: &str, parent: &str, data: Vec<u8>) -> Result<(), libc::c_int> {
        let vault = self.vault.clone();
        let p = path.to_string();
        let fut = async move {
            let mut g = vault.write().await;
            g.fs().add(&abs(&p), Cursor::new(data)).await?;
            g.save().await.map_err(save_err)?;
            Ok(())
        };
        self.commit(&[path, parent], fut)
    }

    /// Shared body for `unlink` + `rmdir`.
    fn remove(&mut self, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        if self.read_only {
            reply.error(libc::EROFS);
            return;
        }
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let Some(parent_path) = self.path_of(parent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let path = build_path(&parent_path, name);
        let vault = self.vault.clone();
        let p = path.clone();
        let fut = async move {
            let mut g = vault.write().await;
            g.fs().rm(&abs(&p)).await?;
            g.save().await.map_err(save_err)?;
            Ok(())
        };
        if let Err(e) = self.commit(&[&path, &parent_path], fut) {
            reply.error(e);
            return;
        }
        self.inodes.lock().unwrap().remove_by_path(&path);
        reply.ok();
    }
}

impl<B: BlobStore, L: VaultLog> Filesystem for FuseFs<B, L> {
    fn init(
        &mut self,
        _req: &Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        tracing::info!("zim-fuse mount initialised");
        Ok(())
    }

    fn destroy(&mut self) {
        tracing::info!("zim-fuse mount destroyed");
    }

    /// Report volume stats. A vault has no fixed size (content is
    /// blake3-addressed and fetched on demand), but macOS **Finder reads
    /// `statfs` and refuses to display a volume that reports 0 blocks /
    /// 0 inodes** — the folder shows empty even though `readdir` works in
    /// the shell. So advertise a large virtual capacity with plenty free.
    fn statfs(&mut self, _req: &Request<'_>, _ino: u64, reply: ReplyStatfs) {
        const BSIZE: u32 = 4096;
        // ~4 TiB of virtual space, essentially all free.
        const BLOCKS: u64 = (4u64 << 40) / BSIZE as u64;
        const INODES: u64 = 1 << 32;
        reply.statfs(
            BLOCKS, // total blocks
            BLOCKS, // free blocks
            BLOCKS, // available blocks (unprivileged)
            INODES, // total inodes
            INODES, // free inodes
            BSIZE,  // block size
            255,    // max filename length
            BSIZE,  // fragment size
        );
    }

    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let Some(name) = name.to_str() else {
            reply.error(libc::ENOENT);
            return;
        };
        if Self::should_filter(name) {
            reply.error(libc::ENOENT);
            return;
        }
        let Some(parent_path) = self.path_of(parent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let path = build_path(&parent_path, name);
        match self.fetch_attr(&path) {
            Some(attr) => {
                let ino = self.inodes.lock().unwrap().get_or_create(&path);
                reply.entry(&ATTR_TTL, &Self::make_attr(ino, &attr), 0);
            }
            None => reply.error(libc::ENOENT),
        }
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let Some(path) = self.path_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        match self.fetch_attr(&path) {
            Some(attr) => reply.attr(&ATTR_TTL, &Self::make_attr(ino, &attr)),
            None => reply.error(libc::ENOENT),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn setattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let Some(path) = self.path_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        // Only truncation is honoured; mtime/mode changes are cache-only.
        if let Some(size) = size {
            if self.read_only {
                reply.error(libc::EROFS);
                return;
            }
            let mut data = self
                .fetch_content(&path)
                .map(|c| c.data.as_ref().clone())
                .unwrap_or_default();
            data.resize(size as usize, 0);
            if let Err(e) = self.write_whole(&path, &InodeTable::parent_path(&path), data) {
                reply.error(e);
                return;
            }
        }
        match self.fetch_attr(&path) {
            Some(attr) => reply.attr(&ATTR_TTL, &Self::make_attr(ino, &attr)),
            None => reply.error(libc::ENOENT),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let Some(path) = self.path_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        let Some(entries) = self.fetch_dir(&path) else {
            reply.error(libc::ENOENT);
            return;
        };
        let mut rows: Vec<(u64, FileType, String)> = vec![
            (ino, FileType::Directory, ".".into()),
            (ino, FileType::Directory, "..".into()),
        ];
        for e in entries {
            let child = build_path(&path, &e.name);
            let cino = self.inodes.lock().unwrap().get_or_create(&child);
            let kind = if e.is_dir {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            rows.push((cino, kind, e.name));
        }
        for (i, (cino, kind, name)) in rows.into_iter().enumerate().skip(offset as usize) {
            // `i + 1` is the next offset the kernel resumes from.
            if reply.add(cino, (i + 1) as i64, kind, &name) {
                break; // buffer full
            }
        }
        reply.ok();
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        // An in-flight write buffer is the freshest view.
        {
            let buffers = self.write_buffers.lock().unwrap();
            if let Some(buf) = buffers.get(&fh) {
                let start = (offset as usize).min(buf.data.len());
                let end = (start + size as usize).min(buf.data.len());
                reply.data(&buf.data[start..end]);
                return;
            }
        }
        let Some(path) = self.path_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        match self.fetch_content(&path) {
            Some(content) => {
                let start = (offset as usize).min(content.data.len());
                let end = (start + size as usize).min(content.data.len());
                reply.data(&content.data[start..end]);
            }
            None => reply.error(libc::ENOENT),
        }
    }

    fn open(&mut self, _req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        let write = flags & (libc::O_WRONLY | libc::O_RDWR) != 0;
        if write && self.read_only {
            reply.error(libc::EROFS);
            return;
        }
        let fh = self.next_handle();
        if write {
            // Seed the buffer with current content so partial writes modify
            // the right base (whole-file replace on flush).
            let seed = self
                .path_of(ino)
                .and_then(|p| self.fetch_content(&p))
                .map(|c| c.data.as_ref().clone())
                .unwrap_or_default();
            self.write_buffers.lock().unwrap().insert(
                fh,
                WriteBuffer {
                    data: seed,
                    dirty: false,
                },
            );
        }
        reply.opened(fh, 0);
    }

    fn write(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        if self.read_only {
            reply.error(libc::EROFS);
            return;
        }
        let mut buffers = self.write_buffers.lock().unwrap();
        let buf = buffers.entry(fh).or_insert(WriteBuffer {
            data: Vec::new(),
            dirty: false,
        });
        let start = offset as usize;
        let end = start + data.len();
        if buf.data.len() < end {
            buf.data.resize(end, 0);
        }
        buf.data[start..end].copy_from_slice(data);
        buf.dirty = true;
        reply.written(data.len() as u32);
    }

    fn flush(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        _lock_owner: u64,
        reply: ReplyEmpty,
    ) {
        let Some(path) = self.path_of(ino) else {
            reply.error(libc::ENOENT);
            return;
        };
        let payload = {
            let mut buffers = self.write_buffers.lock().unwrap();
            match buffers.get_mut(&fh) {
                Some(buf) if buf.dirty => {
                    buf.dirty = false;
                    Some(buf.data.clone())
                }
                _ => None,
            }
        };
        let Some(data) = payload else {
            reply.ok();
            return;
        };
        match self.write_whole(&path, &InodeTable::parent_path(&path), data) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }

    fn release(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        self.write_buffers.lock().unwrap().remove(&fh);
        reply.ok();
    }

    fn create(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        if self.read_only {
            reply.error(libc::EROFS);
            return;
        }
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let Some(parent_path) = self.path_of(parent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let path = build_path(&parent_path, name);
        if let Err(e) = self.write_whole(&path, &parent_path, Vec::new()) {
            reply.error(e);
            return;
        }
        let ino = self.inodes.lock().unwrap().get_or_create(&path);
        let attr = CachedAttr {
            size: 0,
            is_dir: false,
            mime_type: None,
            mtime: now_unix(),
        };
        let fh = self.next_handle();
        self.write_buffers.lock().unwrap().insert(
            fh,
            WriteBuffer {
                data: Vec::new(),
                dirty: false,
            },
        );
        reply.created(&ATTR_TTL, &Self::make_attr(ino, &attr), 0, fh, 0);
    }

    fn mkdir(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        if self.read_only {
            reply.error(libc::EROFS);
            return;
        }
        let Some(name) = name.to_str() else {
            reply.error(libc::EINVAL);
            return;
        };
        let Some(parent_path) = self.path_of(parent) else {
            reply.error(libc::ENOENT);
            return;
        };
        let path = build_path(&parent_path, name);
        let vault = self.vault.clone();
        let p = path.clone();
        let fut = async move {
            let mut g = vault.write().await;
            g.fs().mkdir(&abs(&p), false).await?;
            g.save().await.map_err(save_err)?;
            Ok(())
        };
        if let Err(e) = self.commit(&[&path, &parent_path], fut) {
            reply.error(e);
            return;
        }
        let ino = self.inodes.lock().unwrap().get_or_create(&path);
        let attr = CachedAttr {
            size: 0,
            is_dir: true,
            mime_type: None,
            mtime: now_unix(),
        };
        reply.entry(&ATTR_TTL, &Self::make_attr(ino, &attr), 0);
    }

    fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        self.remove(parent, name, reply);
    }

    fn rmdir(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        self.remove(parent, name, reply);
    }

    fn rename(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        if self.read_only {
            reply.error(libc::EROFS);
            return;
        }
        let (Some(name), Some(newname)) = (name.to_str(), newname.to_str()) else {
            reply.error(libc::EINVAL);
            return;
        };
        let (Some(pp), Some(npp)) = (self.path_of(parent), self.path_of(newparent)) else {
            reply.error(libc::ENOENT);
            return;
        };
        let from = build_path(&pp, name);
        let to = build_path(&npp, newname);
        let vault = self.vault.clone();
        let (f, t) = (from.clone(), to.clone());
        let fut = async move {
            let mut g = vault.write().await;
            g.fs().mv(&abs(&f), &abs(&t)).await?;
            g.save().await.map_err(save_err)?;
            Ok(())
        };
        if let Err(e) = self.commit(&[&from, &to, &pp, &npp], fut) {
            reply.error(e);
            return;
        }
        self.inodes.lock().unwrap().rename(&from, &to);
        reply.ok();
    }

    // xattr: accept-and-drop setxattr so macOS `mv`/`cp` (which set
    // com.apple.* attrs) don't fail; report nothing for the rest.
    fn setxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _value: &[u8],
        _flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    fn getxattr(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _name: &OsStr,
        _size: u32,
        reply: ReplyXattr,
    ) {
        // "No such attribute." On macOS this MUST be ENOATTR (93); ENODATA (96)
        // is a distinct errno there, and Finder treats a non-ENOATTR result on
        // `com.apple.ResourceFork`/`FinderInfo` as a read error and hides the
        // file (directories are probed less and still show). On Linux the two
        // are the same value.
        #[cfg(target_os = "macos")]
        reply.error(libc::ENOATTR);
        #[cfg(not(target_os = "macos"))]
        reply.error(libc::ENODATA);
    }

    fn listxattr(&mut self, _req: &Request<'_>, _ino: u64, size: u32, reply: ReplyXattr) {
        if size == 0 {
            reply.size(0);
        } else {
            reply.data(&[]);
        }
    }
}

/// Mount `fs` at `mountpoint` in a background session. Dropping the returned
/// [`fuser::BackgroundSession`] unmounts. Builds platform-appropriate mount
/// options (macOS volname/local/noappledouble; AutoUnmount everywhere).
pub fn spawn_mount<B: BlobStore, L: VaultLog>(
    fs: FuseFs<B, L>,
    mountpoint: &std::path::Path,
    read_only: bool,
    volname: &str,
) -> std::io::Result<fuser::BackgroundSession> {
    use fuser::MountOption;
    let mut opts = vec![
        MountOption::FSName("zim".to_string()),
        MountOption::AutoUnmount,
    ];
    if read_only {
        opts.push(MountOption::RO);
    }
    #[cfg(target_os = "macos")]
    {
        opts.push(MountOption::CUSTOM(format!("volname={volname}")));
        opts.push(MountOption::CUSTOM("local".to_string()));
        opts.push(MountOption::CUSTOM("noappledouble".to_string()));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = volname;
    }
    fuser::spawn_mount2(fs, mountpoint, &opts)
}
