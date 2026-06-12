//! Conflict-file resolver: LWW picks the winner for the real path; the loser
//! gets renamed to `<path>@<short-hash>` so the user can see what was overridden.

use std::path::Path;

use zim_crypto::PublicKey;

use super::{Conflict, ConflictResolver, Resolution, Winner};
use crate::fs::crdt::op::OpKind;
use crate::fs::AbsPath;

const HASH_LEN: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct ConflictFile;

impl ConflictResolver for ConflictFile {
    fn resolve(&self, conflict: &Conflict, _local_peer: &PublicKey) -> Resolution {
        let incoming_wins = conflict.incoming.id > conflict.base.id;

        let loser = if incoming_wins {
            &conflict.base
        } else {
            &conflict.incoming
        };

        let loser_path = make_conflict_path(loser);

        Resolution::ConflictFile {
            winner: if incoming_wins {
                Winner::Incoming
            } else {
                Winner::Base
            },
            loser_path,
        }
    }
}

/// Generate a conflict filename: `<original>@<short-hash-or-timestamp>`.
fn make_conflict_path(loser: &crate::fs::crdt::Op) -> AbsPath {
    let original: &AbsPath = loser.path();
    let version = match &loser.kind {
        OpKind::AddFile { content, .. } => {
            let hash_str = content.hash().to_string();
            hash_str.chars().take(HASH_LEN).collect::<String>()
        }
        _ => loser.id.timestamp.to_string(),
    };

    let original_path: &Path = original.as_ref();
    let file_name = original_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let conflict_name = format!("{file_name}@{version}");

    let parent = original_path.parent().unwrap_or(Path::new("/"));
    AbsPath::from_abs(parent.join(conflict_name))
}
