//! Autosave bookkeeping, free of timers and I/O so it tests natively.
//!
//! The editor reports every change with [`SaveQueue::edit`]; the page
//! asks for a write with [`SaveQueue::flush`] (when the autosave timer fires, or
//! right away on a file switch / tab hide / Cmd+S) and drives writes with
//! [`SaveQueue::start`] + [`SaveQueue::finish`]. At most one write is in
//! flight; edits made meanwhile coalesce into the next one, and nothing
//! typed is ever dropped by a save finishing.

use std::collections::BTreeMap;

#[derive(Default)]
pub struct SaveQueue {
    /// Latest unsaved text per path, not yet handed to a write.
    drafts: BTreeMap<String, String>,
    /// The write in progress: (path, text).
    in_flight: Option<(String, String)>,
    /// A flush was asked for and drafts remain to drain.
    flush_wanted: bool,
}

impl SaveQueue {
    /// The editor changed `path` to `text`. Supersedes any earlier
    /// unsaved text for that path.
    pub fn edit(&mut self, path: &str, text: String) {
        self.drafts.insert(path.to_string(), text);
    }

    /// Write every draft as soon as possible.
    pub fn flush(&mut self) {
        self.flush_wanted = !self.drafts.is_empty();
    }

    /// The next write to run, if a flush is wanted and none is in flight.
    pub fn start(&mut self) -> Option<(String, String)> {
        if self.in_flight.is_some() || !self.flush_wanted {
            return None;
        }
        let next = self.drafts.pop_first();
        self.flush_wanted = !self.drafts.is_empty();
        self.in_flight.clone_from(&next);
        next
    }

    /// The in-flight write is done. A failed write's text goes back into
    /// the queue unless the user has typed past it, and draining stops
    /// until the next flush (no hot retry loop).
    pub fn finish(&mut self, ok: bool) {
        let Some((path, text)) = self.in_flight.take() else {
            return;
        };
        if !ok {
            self.drafts.entry(path).or_insert(text);
            self.flush_wanted = false;
        }
    }

    /// The newest text for `path` that isn't saved yet.
    pub fn unsaved(&self, path: &str) -> Option<&str> {
        self.drafts.get(path).map(String::as_str).or_else(|| {
            self.in_flight
                .as_ref()
                .filter(|(p, _)| p == path)
                .map(|(_, t)| t.as_str())
        })
    }

    pub fn is_dirty(&self, path: &str) -> bool {
        self.unsaved(path).is_some()
    }

    /// Nothing is being written and nothing is waiting to be: either all
    /// drafts are saved, or the last write failed and left them parked.
    pub fn is_settled(&self) -> bool {
        self.in_flight.is_none() && !self.flush_wanted
    }

    /// `path` (and everything under it) was deleted: its drafts go too.
    pub fn discard(&mut self, path: &str) {
        let dir = format!("{}/", path.trim_end_matches('/'));
        self.drafts.retain(|p, _| p != path && !p.starts_with(&dir));
        self.flush_wanted = self.flush_wanted && !self.drafts.is_empty();
    }

    /// Drop every draft (the vault they belong to was closed).
    pub fn clear(&mut self) {
        self.drafts.clear();
        self.flush_wanted = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "/alice/notes.md";
    const TODO: &str = "/alice/todo.md";

    #[test]
    fn alice_keeps_typing_while_save_in_flight_and_nothing_is_lost() {
        let mut q = SaveQueue::default();

        // Alice types; the autosave timer fires and flushes.
        q.edit(NOTE, "hello".into());
        q.flush();
        let write = q.start().expect("a write starts");
        assert_eq!(write, (NOTE.to_string(), "hello".to_string()));

        // She keeps typing while it's on the wire: no second write starts.
        q.edit(NOTE, "hello wor".into());
        q.edit(NOTE, "hello world".into());
        assert!(q.start().is_none());

        // The first write lands; her newer text is still unsaved.
        q.finish(true);
        assert_eq!(q.unsaved(NOTE), Some("hello world"));

        // Her next pause flushes it as one coalesced write.
        q.flush();
        assert_eq!(
            q.start(),
            Some((NOTE.to_string(), "hello world".to_string()))
        );
        q.finish(true);
        assert!(!q.is_dirty(NOTE));
        assert!(q.is_settled());
    }

    #[test]
    fn nothing_is_written_until_the_timer_flushes() {
        let mut q = SaveQueue::default();

        q.edit(NOTE, "h".into());
        q.edit(NOTE, "hi".into());

        assert!(q.start().is_none());
        assert!(q.is_dirty(NOTE));
        q.flush();
        assert_eq!(q.start(), Some((NOTE.to_string(), "hi".to_string())));
    }

    #[test]
    fn alice_switches_notes_and_both_drafts_are_saved() {
        let mut q = SaveQueue::default();

        // Alice edits one note, opens another (flush), edits that too.
        q.edit(NOTE, "draft one".into());
        q.flush();
        let first = q.start().expect("first note saves");
        q.edit(TODO, "draft two".into());
        q.flush();

        // The second waits for the first, then goes without another flush.
        assert!(q.start().is_none());
        assert_eq!(first.0, NOTE);
        q.finish(true);
        assert_eq!(q.start(), Some((TODO.to_string(), "draft two".to_string())));
        q.finish(true);
        assert!(q.is_settled());
        assert!(!q.is_dirty(NOTE) && !q.is_dirty(TODO));
    }

    #[test]
    fn a_failed_save_keeps_the_text_for_the_next_try() {
        let mut q = SaveQueue::default();

        q.edit(NOTE, "offline words".into());
        q.flush();
        q.start();
        q.finish(false);

        // The text is parked, not retried in a loop.
        assert_eq!(q.unsaved(NOTE), Some("offline words"));
        assert!(q.start().is_none());
        assert!(q.is_settled());

        // The next flush (Cmd+S, another keystroke) retries it.
        q.flush();
        assert_eq!(
            q.start(),
            Some((NOTE.to_string(), "offline words".to_string()))
        );
    }

    #[test]
    fn a_failed_save_does_not_clobber_newer_typing() {
        let mut q = SaveQueue::default();

        q.edit(NOTE, "old".into());
        q.flush();
        q.start();
        q.edit(NOTE, "newer".into());
        q.finish(false);

        assert_eq!(q.unsaved(NOTE), Some("newer"));
    }

    #[test]
    fn deleting_a_folder_drops_the_drafts_inside_it() {
        let mut q = SaveQueue::default();

        q.edit(NOTE, "a".into());
        q.edit(TODO, "b".into());
        q.edit("/alice-archive/old.md", "c".into());
        q.flush();
        q.discard("/alice");

        assert!(!q.is_dirty(NOTE) && !q.is_dirty(TODO));
        assert_eq!(
            q.start(),
            Some(("/alice-archive/old.md".to_string(), "c".to_string()))
        );
    }

    #[test]
    fn closing_the_vault_settles_once_the_last_write_lands() {
        let mut q = SaveQueue::default();

        q.edit(NOTE, "a".into());
        q.flush();
        q.start();
        q.edit(TODO, "b".into());
        q.clear();

        assert!(!q.is_settled());
        q.finish(true);
        assert!(q.is_settled());
        assert!(q.start().is_none());
    }
}
