//! CRDT op log for the filesystem.
//!
//! Tracks path operations ([`OpKind::AddFile`], [`OpKind::Mkdir`],
//! [`OpKind::Remove`], [`OpKind::Mv`]) across peers with causal ordering
//! via Lamport timestamps in [`OpId`].
//!
//! Two ways to merge two logs:
//!
//! - [`OpsLog::merge`] — simple union: pull in every [`Op`] we don't
//!   already have. Conflicts aren't resolved; the destination ends up
//!   with both sides' ops and applies them in [`OpId`] order.
//! - [`OpsLog::merge_with_resolver`] — pluggable LWW-with-resolver.
//!   When two ops target the same path and one is destructive, the
//!   resolver decides what to keep. The default resolver is
//!   [`ConflictFile`], which renames the loser to a sidecar so the user
//!   can see what was overridden.

pub mod op;
pub mod resolver;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::fs::AbsPath;
use zim_crypto::PublicKey;

pub use op::{Op, OpId, OpKind};
pub use resolver::{
    conflicts_with_mv_source, operations_conflict, Conflict, ConflictFile, ConflictResolver,
    MergeResult, Resolution, ResolvedConflict,
};

/// Append-only log of filesystem operations, ordered by [`OpId`].
///
/// Carries a Lamport `clock` so newly recorded ops stay monotonic across
/// load/save cycles. Persisted inside the [`Manifest`](crate::fs::Manifest)
/// (encrypted) and seeded from `manifest.ops_clock()` at load time via
/// [`OpsLog::with_clock`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsLog {
    operations: BTreeMap<OpId, Op>,
    #[serde(default)]
    clock: u64,
}

impl OpsLog {
    /// An empty log with the clock at zero. Used for genesis vaults.
    pub fn new() -> Self {
        Self {
            operations: BTreeMap::new(),
            clock: 0,
        }
    }

    /// An empty log seeded with a starting Lamport clock value. Used at
    /// load time to preserve timestamp monotonicity across sessions —
    /// without this, two clients restoring from the same manifest could
    /// re-issue identical timestamps and collide.
    pub fn with_clock(clock: u64) -> Self {
        Self {
            operations: BTreeMap::new(),
            clock,
        }
    }

    /// A log containing exactly `op`. The clock is set to `op.id.timestamp`
    /// so future `record` calls remain monotonic.
    pub fn from_op(op: &Op) -> Self {
        let mut log = Self::new();
        log.operations.insert(op.id.clone(), op.clone());
        log.clock = op.id.timestamp;
        log
    }

    /// True if the log holds no ops.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Number of ops in the log.
    pub fn len(&self) -> usize {
        self.operations.len()
    }

    /// The current Lamport clock value. Persisted into the manifest at
    /// save time so a future `load` can seed a fresh `OpsLog` with it.
    pub fn clock(&self) -> u64 {
        self.clock
    }

    /// Record a new operation authored by `peer_id`. Advances the
    /// Lamport clock by one, mints an [`OpId`], and inserts the op.
    pub fn record(&mut self, peer_id: PublicKey, kind: OpKind) {
        self.clock += 1;
        let id = OpId {
            timestamp: self.clock,
            peer_id,
        };
        let op = Op {
            id: id.clone(),
            kind,
        };
        self.operations.insert(id, op);
    }

    /// Simple merge: append all ops from `other` that we don't have.
    /// No conflict detection. Returns count of ops added.
    pub fn merge(&mut self, other: &OpsLog) -> usize {
        let mut added = 0;
        for (id, op) in &other.operations {
            if !self.operations.contains_key(id) {
                self.operations.insert(id.clone(), op.clone());
                added += 1;
                if id.timestamp > self.clock {
                    self.clock = id.timestamp;
                }
            }
        }
        added
    }

    /// Merge with conflict detection + resolution.
    pub fn merge_with_resolver<R: ConflictResolver>(
        &mut self,
        other: &OpsLog,
        resolver: &R,
        local_peer: &PublicKey,
    ) -> MergeResult {
        let mut result = MergeResult::new();

        // Group incoming ops by path for conflict detection
        let mut conflicts_by_path: BTreeMap<AbsPath, Vec<(&OpId, &Op)>> = BTreeMap::new();

        for (id, op) in &other.operations {
            if self.operations.contains_key(id) {
                continue;
            }
            // Check against existing ops for conflicts
            let path = op.path().clone();
            let has_conflict = self
                .operations
                .values()
                .any(|existing| operations_conflict(existing, op));

            if has_conflict {
                conflicts_by_path.entry(path).or_default().push((id, op));
            } else {
                self.operations.insert(id.clone(), op.clone());
                result.operations_added += 1;
                if id.timestamp > self.clock {
                    self.clock = id.timestamp;
                }
            }
        }

        // Resolve conflicts
        for (path, incoming_ops) in conflicts_by_path {
            for (_id, incoming_op) in incoming_ops {
                // Find the conflicting base op
                let base_op = self
                    .operations
                    .values()
                    .find(|existing| operations_conflict(existing, incoming_op))
                    .cloned();

                let Some(base) = base_op else {
                    // No conflict after all (race with prior resolution)
                    self.operations
                        .insert(incoming_op.id.clone(), incoming_op.clone());
                    result.operations_added += 1;
                    continue;
                };

                let conflict = Conflict::new(path.clone(), base, incoming_op.clone());
                let resolution = resolver.resolve(&conflict, local_peer);

                match &resolution {
                    Resolution::UseIncoming => {
                        self.operations
                            .insert(incoming_op.id.clone(), incoming_op.clone());
                        result.operations_added += 1;
                    }
                    Resolution::UseBase => {
                        // Keep existing, don't add incoming
                    }
                    Resolution::KeepBoth => {
                        self.operations
                            .insert(incoming_op.id.clone(), incoming_op.clone());
                        result.operations_added += 1;
                        result.unresolved_conflicts.push(conflict.clone());
                    }
                    Resolution::ConflictFile { winner, loser_path } => {
                        // Winner takes the real path
                        match winner {
                            resolver::Winner::Incoming => {
                                self.operations
                                    .insert(incoming_op.id.clone(), incoming_op.clone());
                                result.operations_added += 1;
                            }
                            resolver::Winner::Base => {
                                // base already in log
                            }
                        }
                        // Record the loser as an AddFile at the conflict path
                        // (so the user can see what was overridden)
                        let loser = match winner {
                            resolver::Winner::Incoming => &conflict.base,
                            resolver::Winner::Base => incoming_op,
                        };
                        if let OpKind::AddFile {
                            content,
                            secret,
                            plaintext_hash,
                            ..
                        } = &loser.kind
                        {
                            let conflict_op = Op {
                                id: loser.id.clone(),
                                kind: OpKind::AddFile {
                                    path: loser_path.clone(),
                                    content: content.clone(),
                                    secret: secret.clone(),
                                    plaintext_hash: *plaintext_hash,
                                },
                            };
                            self.operations.insert(conflict_op.id.clone(), conflict_op);
                            result.operations_added += 1;
                        }
                    }
                }

                result.conflicts_resolved.push(ResolvedConflict {
                    conflict,
                    resolution,
                });
            }
        }

        result
    }

    pub fn operations(&self) -> &BTreeMap<OpId, Op> {
        &self.operations
    }

    /// Find the latest operation affecting `path`.
    pub fn resolve_path(&self, path: &AbsPath) -> Option<&Op> {
        self.operations
            .values()
            .filter(|op| op.path() == path)
            .max_by_key(|op| &op.id)
    }

    pub fn clear_preserving_clock(&mut self) {
        self.operations.clear();
    }

    /// Rebuild the Lamport clock from the max timestamp in the log.
    /// Used after deserializing an OpLog that may not have a stored clock.
    pub fn rebuild_clock(&mut self) {
        self.clock = self
            .operations
            .keys()
            .map(|id| id.timestamp)
            .max()
            .unwrap_or(0);
    }

    /// Resolve to the latest operation per path. Returns path → Op mapping
    /// representing the "current state" implied by the log.
    pub fn resolve_all(&self) -> BTreeMap<AbsPath, &Op> {
        let mut resolved: BTreeMap<AbsPath, &Op> = BTreeMap::new();
        for op in self.operations.values() {
            let path = op.path().clone();
            match resolved.get(&path) {
                Some(existing) if existing.id > op.id => {}
                _ => {
                    resolved.insert(path, op);
                }
            }
        }
        resolved
    }
}

impl Default for OpsLog {
    fn default() -> Self {
        Self::new()
    }
}
