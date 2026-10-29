//! Pure decisions behind the `.md` editor, free of the DOM so they test natively.

/// `.md` notes get the WYSIWYG editor; every other text file stays a textarea.
pub fn is_markdown(path: &str) -> bool {
    path.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
}

/// Milkdown re-serializes the markdown it loads (list markers, escaping),
/// so its output right after mount is the baseline: only text that differs
/// from it is a user edit worth saving. Without this, just opening a note
/// would autosave a rewritten file.
pub fn is_edit(baseline: Option<&str>, markdown: &str) -> bool {
    baseline.is_some_and(|b| b != markdown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bob_notes_are_markdown_but_his_scripts_and_extensionless_files_are_not() {
        assert!(is_markdown("notes/todo.md"));
        assert!(is_markdown("README.MD"));
        assert!(!is_markdown("run.sh"));
        assert!(!is_markdown("LICENSE"));
        assert!(!is_markdown("docs.md/plain"));
    }

    #[test]
    fn alice_opens_a_note_milkdown_rewrites_and_nothing_is_saved() {
        // Milkdown turns `* a` into `- a` on load: that's the baseline.
        let baseline = Some("- a\n");
        assert!(!is_edit(baseline, "- a\n"));
    }

    #[test]
    fn alice_types_a_line_and_the_edit_is_emitted() {
        assert!(is_edit(Some("- a\n"), "- a\n- b\n"));
    }

    #[test]
    fn alice_undoes_her_typing_back_to_the_baseline_and_nothing_is_emitted() {
        assert!(!is_edit(Some("hello\n"), "hello\n"));
    }

    #[test]
    fn updates_during_mount_before_a_baseline_exists_are_not_edits() {
        assert!(!is_edit(None, "anything\n"));
    }
}
