//! One row of a directory listing — the shape every front end shares
//! (daemon `ls` reply, browser SDK, desktop IPC).

use serde::{Deserialize, Serialize};

/// What a directory entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    // Declared first so sorting by kind lists directories before files.
    Dir,
    File,
}

/// One entry of a directory listing: its name and kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
}
