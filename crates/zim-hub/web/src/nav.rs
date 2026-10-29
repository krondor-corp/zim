//! Narrow-screen navigation state, kept platform-free so it unit-tests
//! natively.

/// Whether the narrow-screen layout shows the detail pane (an open or
/// syncing file, or an error to read) instead of the file tree.
pub fn shows_detail(file_open: bool, syncing: bool, has_error: bool) -> bool {
    file_open || syncing || has_error
}

#[cfg(test)]
mod tests {
    use super::shows_detail;

    #[test]
    fn alice_sees_the_tree_until_she_opens_a_file() {
        // Nothing open, nothing syncing, no error: list view.
        assert!(!shows_detail(false, false, false));
        // She taps a note: detail view.
        assert!(shows_detail(true, false, false));
    }

    #[test]
    fn bob_stays_on_the_detail_pane_while_a_file_syncs_or_fails() {
        // A file still on its way to the hub shows its syncing notice.
        assert!(shows_detail(false, true, false));
        // A failed open keeps the error on screen until he goes back.
        assert!(shows_detail(false, false, true));
    }
}
