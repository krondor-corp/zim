//! FUSE filesystem for Zim vaults.
//!
//! [`FuseFs`] exposes a [`zim_core::vault::Vault`] as a local POSIX directory
//! via the `fuser` crate. Unlike the daemon's HTTP path — which re-opens the
//! vault per request to avoid forking the head — a mount holds **one
//! long-lived [`Vault`](zim_core::vault::Vault) behind a write lock** and
//! serializes every mutation through it. That's both the performance story
//! (no manifest re-read/decrypt per `getattr`) and the correctness story: a
//! single in-process writer advances the chain linearly instead of racing
//! concurrent opens into a silent fork.
//!
//! Reconciliation with *external* writers (the daemon's own HTTP API, or a
//! remote sync advancing the head) is the [`MountManager`]'s job in the
//! daemon; this crate is the filesystem mechanism.
//!
//! The mount machinery is behind the `mount` feature so the workspace build
//! doesn't require libfuse/macFUSE. The `inode_table` is always available.

pub mod inode_table;

#[cfg(feature = "mount")]
mod cache;
#[cfg(feature = "mount")]
mod fuse_fs;

#[cfg(feature = "mount")]
pub use cache::{
    CacheStats, CachedAttr, CachedContent, CachedDirEntry, FileCache, FileCacheConfig,
};
#[cfg(feature = "mount")]
pub use fuse_fs::{spawn_mount, FuseFs};
#[cfg(feature = "mount")]
pub use fuser::BackgroundSession;
