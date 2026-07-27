//! The filesystem layer.
//!
//! Zim's storage is organized into *vaults* — versioned, signed,
//! shareable entities identified by a manifest chain. This module
//! implements the in-memory filesystem that backs a single vault
//! version: the directory tree, the per-entry encryption, the op log,
//! and the persistence path through [`ContentStore`](content_store::ContentStore).
//!
//! The vault concept (chain bookkeeping, sharing, sync) is
//! built *on top of* this layer in `zim-protocol` and higher.
//!
//! # Core types
//!
//! - [`Fs`] — the in-memory filesystem implementation for a vault.
//!   Holds the decrypted root [`Dir`], the [`Manifest`], and the
//!   pending [`OpsLog`]. Mutated through [`Fs::add`], [`Fs::mkdir`],
//!   [`Fs::rm`], [`Fs::mv`]; persisted via [`Fs::save`].
//! - [`Manifest`] — the signed top-level record of a vault version: id,
//!   name, [`Shares`], [`Pins`], inline [`Metadata`](content_store::Metadata),
//!   history pointer.
//! - [`Dir`] — a directory's children map (`name → Entry`). Its encoded
//!   form is what ships in the metadata pack.
//! - [`Entry`] — what a [`Dir`] stores for each child. Either an
//!   `Entry::File` (link + secret + mime + metadata) or an
//!   `Entry::Dir` (link + secret — dereference via
//!   [`ContentStore`](content_store::ContentStore)).
//! - [`AbsPath`] — a validated absolute path. All public `Fs` APIs take
//!   one. See [`AbsPath::split`] for the common (parent, name) breakdown.
//! - [`ContentStore`](content_store::ContentStore) — the fs-layer's
//!   view of storage. Splits the metadata pack (encrypted dir bodies,
//!   ship inline with the manifest) from the inner blob store (file
//!   content, ops log, manifest history).
//!
//! # CRDT layer
//!
//! Concurrent edits across peers are reconciled via the [`crdt`] module:
//! [`OpsLog`] of [`Op`]s ordered by [`OpId`] (Lamport timestamp + peer
//! id), with pluggable [`ConflictResolver`]s — see [`ConflictFile`] for
//! the default LWW-with-conflict-file strategy.

mod abs_path;
pub mod content_store;
pub mod crdt;
mod entry;
mod fs_inner;
mod manifest;
mod maybe_mime;
mod pins;
mod share;
#[cfg(test)]
mod tests;

pub use abs_path::AbsPath;

pub use crdt::{
    conflicts_with_mv_source, operations_conflict, Conflict, ConflictFile, ConflictResolver,
    MergeResult, Op, OpId, OpKind, OpsLog, Resolution, ResolvedConflict,
};
pub use entry::{Dir, Entry};
pub use fs_inner::{Fs, FsError, FsInner};
pub use manifest::{Manifest, ManifestError, Shares};
pub use pins::Pins;
pub use share::Share;
