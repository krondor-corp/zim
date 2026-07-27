//! FUSE filesystem for Zim vaults (behind the `fuse` feature).
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
//! remote sync advancing the head) is [`crate::mount::MountManager`]'s job;
//! this module is the filesystem mechanism. It stays deliberately
//! daemon-agnostic — everything here is generic over `Vault<B, L>` and
//! imports nothing from the daemon/CLI/HTTP layers.

pub mod inode_table;

mod cache;
mod fuse_fs;

pub use cache::{
    CacheStats, CachedAttr, CachedContent, CachedDirEntry, FileCache, FileCacheConfig,
};
pub use fuse_fs::{spawn_mount, FuseFs};
pub use fuser::BackgroundSession;
