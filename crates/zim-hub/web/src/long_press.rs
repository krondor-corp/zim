//! Touch long-press: the touch screen's stand-in for right-click. Touch
//! browsers (iOS above all) don't fire `contextmenu`, so a finger held
//! still for [`HOLD_MS`] opens the same menu. Pure state; the DOM glue
//! (touch events, the hold timer) lives with the tree pane.

/// How long a finger stays down before the press counts.
pub const HOLD_MS: i32 = 450;
/// How far (px, either axis) a finger may drift and still be holding —
/// past it, the touch is a scroll.
pub const SLOP_PX: i32 = 10;

/// One finger's press, from touchstart to touchend.
#[derive(Debug, Default)]
pub struct Press {
    /// Bumped per touch, so a stale hold timer can't fire a newer press.
    generation: u32,
    /// Where the finger went down, while it's still holding.
    origin: Option<(i32, i32)>,
    /// The hold elapsed: this touch opened a menu, so its tap is swallowed.
    fired: bool,
}

impl Press {
    /// A finger went down at `(x, y)`. Returns the press's generation,
    /// handed back to [`Press::elapsed`] when the hold timer runs out.
    pub fn start(&mut self, x: i32, y: i32) -> u32 {
        self.generation = self.generation.wrapping_add(1);
        self.origin = Some((x, y));
        self.fired = false;
        self.generation
    }

    /// The finger moved to `(x, y)`; drifting past [`SLOP_PX`] drops it.
    pub fn moved(&mut self, x: i32, y: i32) {
        if let Some((ox, oy)) = self.origin {
            if (x - ox).abs() > SLOP_PX || (y - oy).abs() > SLOP_PX {
                self.origin = None;
            }
        }
    }

    /// A second finger, or the browser took the touch over.
    pub fn cancel(&mut self) {
        self.origin = None;
    }

    /// Press `generation`'s hold timer ran out: where to open the menu,
    /// if that same finger is still down and still.
    pub fn elapsed(&mut self, generation: u32) -> Option<(i32, i32)> {
        if generation != self.generation {
            return None;
        }
        let at = self.origin.take()?;
        self.fired = true;
        Some(at)
    }

    /// The finger lifted. True when this press opened a menu — the
    /// caller swallows the tap so the row doesn't also open.
    pub fn end(&mut self) -> bool {
        self.origin = None;
        std::mem::take(&mut self.fired)
    }
}

#[cfg(test)]
mod tests {
    use super::{Press, SLOP_PX};

    #[test]
    fn alice_holds_a_row_and_gets_its_menu() {
        let mut press = Press::default();

        // Alice puts her finger on a note and keeps it there.
        let generation = press.start(40, 200);
        press.moved(42, 199);

        // The hold runs out: the menu opens where she pressed.
        assert_eq!(press.elapsed(generation), Some((40, 200)));
        // Lifting her finger doesn't also open the note.
        assert!(press.end());
    }

    #[test]
    fn alice_taps_a_row_and_just_opens_it() {
        let mut press = Press::default();
        let generation = press.start(40, 200);

        // She lifts before the hold runs out: a plain tap.
        assert!(!press.end());
        assert_eq!(press.elapsed(generation), None);
    }

    #[test]
    fn bob_scrolls_the_tree_without_opening_a_menu() {
        let mut press = Press::default();
        let generation = press.start(40, 200);

        // Bob's finger drags up the list — that's a scroll.
        press.moved(40, 200 - SLOP_PX - 1);

        assert_eq!(press.elapsed(generation), None);
        assert!(!press.end());
    }

    #[test]
    fn a_pinch_cancels_the_press() {
        let mut press = Press::default();
        let generation = press.start(40, 200);

        // A second finger lands.
        press.cancel();

        assert_eq!(press.elapsed(generation), None);
    }

    #[test]
    fn an_old_taps_timer_cant_fire_alices_next_press() {
        let mut press = Press::default();

        // Alice taps once, then quickly presses again.
        let first = press.start(40, 200);
        press.end();
        let second = press.start(40, 260);

        // The first tap's timer runs out mid second press: nothing.
        assert_eq!(press.elapsed(first), None);
        // The second press's own timer opens its menu.
        assert_eq!(press.elapsed(second), Some((40, 260)));
    }
}
