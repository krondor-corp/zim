//! Zim filesystem: the in-memory bucket representation, content-addressed node DAG, manifest, and CRDT path operations.
//!
//! - **[`fs::Fs`]**: the filesystem handle — open, save, mutate (add/rm/mkdir/mv), publish, merge.
//! - **[`fs::Manifest`]**: bucket metadata (id, name, shares, principals, content pointers).
//! - **[`fs::Node`]**: encrypted DAG node (file or directory tree).
//! - **[`fs::PathOpLog`]**: append-only CRDT log of path mutations, used for sync and conflict resolution.

pub mod fs;

pub use fs::{
    conflicts_with_mv_source, merge_logs, operations_conflict, BaseWins, Conflict, ConflictFile,
    ConflictResolver, ForkOnConflict, Fs, FsError, LastWriteWins, Manifest, ManifestError,
    MergeResult, Node, NodeError, NodeLink, OpId, OpType, PathOpLog, PathOperation, Pins,
    Principal, PrincipalRole, Resolution, ResolvedConflict, Share, Shares,
};
