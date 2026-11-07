//! Narrow-screen navigation state, kept platform-free so it unit-tests
//! natively.

/// Whether the narrow-screen layout shows the detail pane (an open or
/// syncing file, or an error to read) instead of the file tree.
pub fn shows_detail(file_open: bool, syncing: bool, has_error: bool) -> bool {
    file_open || syncing || has_error
}

/// Which History page the narrow-screen layout shows: the version list,
/// one version's folder, or the file viewer. Each step back clears the
/// state that put it there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryPage {
    Versions,
    Folder,
    File,
}

impl HistoryPage {
    /// `file_shown`: a file previewed or syncing, or an error to read —
    /// all of which live in the viewer pane.
    pub fn of(version_open: bool, file_shown: bool) -> Self {
        if file_shown {
            Self::File
        } else if version_open {
            Self::Folder
        } else {
            Self::Versions
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{shows_detail, HistoryPage};

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

    #[test]
    fn alice_steps_into_an_old_version_and_back_out() {
        // History opens on the version list.
        assert_eq!(HistoryPage::of(false, false), HistoryPage::Versions);
        // She taps v3: its folder fills the screen.
        assert_eq!(HistoryPage::of(true, false), HistoryPage::Folder);
        // She taps a note in it: the viewer, with Restore.
        assert_eq!(HistoryPage::of(true, true), HistoryPage::File);
        // Back closes the file: the folder again.
        assert_eq!(HistoryPage::of(true, false), HistoryPage::Folder);
        // Back again lets go of the version: the list.
        assert_eq!(HistoryPage::of(false, false), HistoryPage::Versions);
    }

    #[test]
    fn bob_reads_a_history_error_in_the_viewer() {
        // The version list failed to load: the error shows in the viewer
        // pane, and back clears it.
        assert_eq!(HistoryPage::of(false, true), HistoryPage::File);
    }
}
