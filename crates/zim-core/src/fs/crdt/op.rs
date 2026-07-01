//! CRDT operation types for the filesystem op log.
//!
//! Defines the three building blocks of the op log:
//!
//! - [`OpId`] — a Lamport-timestamp + peer-id pair that totally orders
//!   ops across all peers.
//! - [`OpKind`] — the kind of mutation an op represents (add a file,
//!   create a dir, remove, move).
//! - [`Op`] — an [`OpId`] paired with an [`OpKind`].
//!
//! The companion [`OpsLog`](super::OpsLog) stores `Op`s keyed by `OpId`.

use crate::linked_data::Hash;
use serde::{Deserialize, Serialize};

use crate::fs::AbsPath;
use crate::linked_data::Link;
use zim_crypto::{PublicKey, Secret};

/// A causal-order identifier. Total order across all peers: Lamport
/// `timestamp` primary, `peer_id` lexicographic secondary. Two `OpId`s
/// with the same timestamp from different peers are deterministically
/// ordered by their public-key bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId {
    /// Lamport timestamp at the moment the op was recorded.
    pub timestamp: u64,
    /// The peer that originated the op.
    pub peer_id: PublicKey,
}

impl PartialOrd for OpId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OpId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.timestamp.cmp(&other.timestamp) {
            std::cmp::Ordering::Equal => self.peer_id.cmp(&other.peer_id),
            ord => ord,
        }
    }
}

/// A causally-ordered filesystem operation: an [`OpId`] plus the
/// [`OpKind`] it performs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    /// The causal-order identifier.
    pub id: OpId,
    /// What the op does.
    pub kind: OpKind,
}

/// What the operation does. Each variant carries exactly the data it
/// needs — no optional fields, no ambiguous `path` that means different
/// things per variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpKind {
    /// Add a file at `path`. The op is self-contained — `content`
    /// addresses the encrypted blob in the shared inner store and
    /// `secret` is the per-file key that decrypts it. Together they
    /// reconstruct the [`Entry::File`](crate::fs::Entry::File) that
    /// goes in the parent directory.
    ///
    /// `secret` rides inside the op log, which is itself encrypted at
    /// rest with the vault secret — so anyone who can read the op log
    /// can already read every file in the vault. Embedding `secret`
    /// here doesn't broaden access; it makes replay self-sufficient.
    AddFile {
        /// Destination path in the tree.
        path: AbsPath,
        /// Link to the encrypted file content in the inner blob store.
        content: Link,
        /// Per-file decryption key.
        secret: Secret,
        /// `blake3(plaintext)` of the body — carried through the log so
        /// replays on remote peers reconstruct
        /// [`Entry::File`](crate::fs::Entry::File) with the same hash
        /// the writer computed. `None` on ops written before this
        /// field existed.
        #[serde(default)]
        plaintext_hash: Option<Hash>,
    },
    /// Create a directory at `path` (idempotent at apply time).
    Mkdir {
        /// Path to create.
        path: AbsPath,
    },
    /// Remove a path. `is_dir` distinguishes file vs directory removal
    /// at replay time without re-reading the tree.
    Remove {
        /// Path to remove.
        path: AbsPath,
        /// `true` when the removed entry was a directory.
        is_dir: bool,
    },
    /// Move/rename a path. `from` disappears; `to` comes into existence.
    Mv {
        /// Source path.
        from: AbsPath,
        /// Destination path.
        to: AbsPath,
    },
}

impl OpKind {
    /// The primary path this operation targets. For `Mv`, this is the
    /// **destination** (the path that comes into existence) — that's
    /// the key conflict detection compares against.
    pub fn path(&self) -> &AbsPath {
        match self {
            OpKind::AddFile { path, .. } => path,
            OpKind::Mkdir { path } => path,
            OpKind::Remove { path, .. } => path,
            OpKind::Mv { to, .. } => to,
        }
    }

    /// True if this op destroys an existing path (`Remove` or `Mv` —
    /// `Mv` removes its source). Used by conflict detection to decide
    /// whether two same-path ops are in tension.
    pub fn is_destructive(&self) -> bool {
        matches!(self, OpKind::Remove { .. } | OpKind::Mv { .. })
    }
}

impl Op {
    /// Convenience: [`OpKind::path`] applied to `self.kind`.
    pub fn path(&self) -> &AbsPath {
        self.kind.path()
    }
}
