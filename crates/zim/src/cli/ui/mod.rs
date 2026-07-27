//! Presentation helpers — plain mode, tables, truncation, durations.
//!
//! Ops never touch this directly; Display impls on Output types do.
//! Split by responsibility (shape borrowed from jig's `cli/ui`):
//!
//! - [`colors`] — semantic text colorers (`ident`, `dim`, `num`, …).
//! - [`output`] — status lines (`success`, `failure`, …) and the
//!   cause-chain error formatter.
//!
//! Everything is re-exported flat, so call sites keep `ui::dim(...)`.
//! All color goes through `owo_colors`' stream-aware guard (honours
//! `NO_COLOR` / non-tty), and the `--plain` flag strips decoration
//! entirely for scripts.

use std::sync::OnceLock;

use comfy_table::{presets, Attribute, Cell, ContentArrangement, Table};

pub mod colors;
pub mod output;

pub use colors::*;
pub use output::*;

// ── Plain mode ─────────────────────────────────────────────────────

static PLAIN_MODE: OnceLock<bool> = OnceLock::new();

/// Set once at startup from `--plain`.
pub fn set_plain(plain: bool) {
    let _ = PLAIN_MODE.set(plain);
}

/// Whether `--plain` was passed (no color, no decoration).
pub fn is_plain() -> bool {
    PLAIN_MODE.get().copied().unwrap_or(false)
}

// ── Tables ─────────────────────────────────────────────────────────

/// Borderless, terminal-width-aware table.
pub fn make_table() -> Table {
    let mut t = Table::new();
    t.load_preset(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic);
    t
}

/// [`make_table`] with a bold header row — the common case.
pub fn table(headers: &[&str]) -> Table {
    let mut t = make_table();
    t.set_header(
        headers
            .iter()
            .map(|h| Cell::new(*h).add_attribute(Attribute::Bold))
            .collect::<Vec<_>>(),
    );
    t
}

// ── Truncation ─────────────────────────────────────────────────────

/// Truncate to `max_len` characters, appending `…` when cut (UTF-8 safe).
pub fn truncate(s: &str, max_len: usize) -> String {
    let n = s.chars().count();
    if n <= max_len {
        s.to_string()
    } else if max_len <= 1 {
        "\u{2026}".to_string()
    } else {
        let head: String = s.chars().take(max_len - 1).collect();
        format!("{head}\u{2026}")
    }
}

// ── Durations ──────────────────────────────────────────────────────

/// Short human duration: `45s`, `3m12s`, `1h5m`.
pub fn format_duration_short(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let (m, s) = (secs / 60, secs % 60);
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m{s}s")
        }
    } else {
        let (h, m) = (secs / 3600, (secs % 3600) / 60);
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h{m}m")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_is_utf8_safe_and_bounded() {
        assert_eq!(truncate("héllo wörld", 5), "héll…");
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("xy", 1), "…");
    }

    #[test]
    fn durations_render_short_forms() {
        assert_eq!(format_duration_short(45), "45s");
        assert_eq!(format_duration_short(192), "3m12s");
        assert_eq!(format_duration_short(300), "5m");
        assert_eq!(format_duration_short(3660), "1h1m");
        assert_eq!(format_duration_short(7200), "2h");
    }
}
