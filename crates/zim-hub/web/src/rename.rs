//! Renaming and moving vault paths: what a typed name may be, and where
//! an open file ends up when it, or a folder above it, moves.

use std::fmt;

/// Why a name typed into the rename dialog can't be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameError {
    Empty,
    /// A name is one path segment; moving between folders is drag & drop.
    HasSlash,
    /// `.` and `..` aren't entries.
    Reserved,
    Unchanged,
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RenameError::Empty => "a name can't be empty",
            RenameError::HasSlash => "a name can't contain '/' (drag it to move it)",
            RenameError::Reserved => "'.' and '..' aren't names",
            RenameError::Unchanged => "that's already its name",
        })
    }
}

/// The path `path` gets when renamed to `name` (surrounding whitespace
/// trimmed), in the same folder.
pub fn renamed(path: &str, name: &str) -> Result<String, RenameError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(RenameError::Empty);
    }
    if name.contains('/') {
        return Err(RenameError::HasSlash);
    }
    if name == "." || name == ".." {
        return Err(RenameError::Reserved);
    }
    let to = match path.rfind('/') {
        Some(0) | None => format!("/{name}"),
        Some(i) => format!("{}/{name}", &path[..i]),
    };
    if to == path {
        return Err(RenameError::Unchanged);
    }
    Ok(to)
}

/// Where `open` lives after `from` moves to `to`: `from` itself, or a
/// folder `from` contains it. `None` when the move doesn't touch it.
pub fn moved_path(open: &str, from: &str, to: &str) -> Option<String> {
    if open == from {
        return Some(to.to_string());
    }
    open.strip_prefix(from)
        .filter(|rest| rest.starts_with('/'))
        .map(|rest| format!("{to}{rest}"))
}

#[cfg(test)]
mod tests {
    use super::{moved_path, renamed, RenameError};

    // ── Typing a new name ─────────────────────────────────────────────

    #[test]
    fn alice_renames_a_note_in_place() {
        assert_eq!(
            renamed("/journal/monday.md", "tuesday.md").as_deref(),
            Ok("/journal/tuesday.md")
        );
        assert_eq!(renamed("/todo.md", "  done.md ").as_deref(), Ok("/done.md"));
    }

    #[test]
    fn alice_renames_a_folder() {
        assert_eq!(
            renamed("/journal/2025", "2026").as_deref(),
            Ok("/journal/2026")
        );
    }

    #[test]
    fn bob_types_names_that_are_not_names() {
        assert_eq!(renamed("/a.md", "   "), Err(RenameError::Empty));
        assert_eq!(renamed("/a.md", "x/b.md"), Err(RenameError::HasSlash));
        assert_eq!(renamed("/a.md", ".."), Err(RenameError::Reserved));
        assert_eq!(renamed("/dir/a.md", "a.md"), Err(RenameError::Unchanged));
    }

    // ── The open file follows ─────────────────────────────────────────

    #[test]
    fn alices_open_note_follows_its_own_rename() {
        assert_eq!(
            moved_path("/a/note.md", "/a/note.md", "/a/n.md").as_deref(),
            Some("/a/n.md")
        );
    }

    #[test]
    fn alices_open_note_follows_its_folders_rename() {
        assert_eq!(
            moved_path("/work/q3/plan.md", "/work", "/jobs").as_deref(),
            Some("/jobs/q3/plan.md")
        );
    }

    #[test]
    fn a_sibling_with_a_longer_name_is_not_inside() {
        // Renaming /work must not drag /workshop/notes.md along.
        assert_eq!(moved_path("/workshop/notes.md", "/work", "/jobs"), None);
        assert_eq!(moved_path("/other.md", "/work", "/jobs"), None);
    }
}
