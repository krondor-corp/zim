//! Conflict resolution for [`OpsLog`](super::OpsLog) merges.
//!
//! Single built-in strategy: [`ConflictFile`]. The trait stays generic
//! so callers (e.g. `Fs::merge_from`) can accept custom resolvers.

mod conflict_file;

pub use conflict_file::ConflictFile;

use zim_crypto::PublicKey;

use super::op::Op;
use crate::fs::AbsPath;

/// Trait for conflict resolution strategies.
pub trait ConflictResolver: std::fmt::Debug + Send + Sync {
    fn resolve(&self, conflict: &Conflict, local_peer: &PublicKey) -> Resolution;
}

/// Two operations on the same path from different peers.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub path: AbsPath,
    pub base: Op,
    pub incoming: Op,
}

impl Conflict {
    pub fn new(path: AbsPath, base: Op, incoming: Op) -> Self {
        Self {
            path,
            base,
            incoming,
        }
    }

    pub fn is_concurrent(&self) -> bool {
        self.base.id.timestamp == self.incoming.id.timestamp
    }

    pub fn crdt_winner(&self) -> &Op {
        if self.incoming.id > self.base.id {
            &self.incoming
        } else {
            &self.base
        }
    }
}

/// Resolution decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    UseBase,
    UseIncoming,
    KeepBoth,
    /// LWW picks the winner; loser is renamed to a conflict file at this path.
    ConflictFile {
        winner: Winner,
        loser_path: AbsPath,
    },
}

/// Which side won.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Winner {
    Base,
    Incoming,
}

/// Result of a merge.
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub operations_added: usize,
    pub conflicts_resolved: Vec<ResolvedConflict>,
    pub unresolved_conflicts: Vec<Conflict>,
}

impl MergeResult {
    pub fn new() -> Self {
        Self {
            operations_added: 0,
            conflicts_resolved: Vec::new(),
            unresolved_conflicts: Vec::new(),
        }
    }

    pub fn has_unresolved(&self) -> bool {
        !self.unresolved_conflicts.is_empty()
    }
}

impl Default for MergeResult {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedConflict {
    pub conflict: Conflict,
    pub resolution: Resolution,
}

/// Check if two operations conflict (same path, different OpIds, at least
/// one destructive or both AddFile).
pub fn operations_conflict(base: &Op, incoming: &Op) -> bool {
    if base.id == incoming.id {
        return false;
    }
    if base.path() != incoming.path() {
        return false;
    }
    let base_destructive = base.kind.is_destructive();
    let incoming_destructive = incoming.kind.is_destructive();

    use super::op::OpKind;
    base_destructive
        || incoming_destructive
        || (matches!(base.kind, OpKind::AddFile { .. })
            && matches!(incoming.kind, OpKind::AddFile { .. }))
}

/// Check if an operation conflicts with a move's source path.
pub fn conflicts_with_mv_source(op: &Op, mv_from: &AbsPath) -> bool {
    op.path() == mv_from
}
