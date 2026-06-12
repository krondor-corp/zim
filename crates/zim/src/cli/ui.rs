//! Presentation helpers — status symbols, colors, tables.
//!
//! Ops never touch this directly. Display impls on Output types do.

use std::sync::OnceLock;

use comfy_table::{presets, ContentArrangement, Table};
use owo_colors::{OwoColorize, Stream};

pub const SUCCESS: &str = "\u{2713}"; // ✓
pub const PROGRESS: &str = "\u{2192}"; // →
pub const FAILURE: &str = "\u{2717}"; // ✗
pub const WARNING: &str = "!";

static PLAIN_MODE: OnceLock<bool> = OnceLock::new();

pub fn set_plain(plain: bool) {
    let _ = PLAIN_MODE.set(plain);
}

pub fn is_plain() -> bool {
    PLAIN_MODE.get().copied().unwrap_or(false)
}

/// Truncate a string to `max_len` characters (append `…` if cut).
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

pub fn success(action: &str, subject: &str) -> String {
    format!(
        "{} {} {}",
        SUCCESS.if_supports_color(Stream::Stdout, |s| s.green().to_string()),
        action.if_supports_color(Stream::Stdout, |s| s.bold().to_string()),
        subject.if_supports_color(Stream::Stdout, |s| s.cyan().to_string()),
    )
}

pub fn failure(action: &str, subject: &str) -> String {
    format!(
        "{} {} {}",
        FAILURE.if_supports_color(Stream::Stdout, |s| s.red().to_string()),
        action.if_supports_color(Stream::Stdout, |s| s.bold().to_string()),
        subject,
    )
}

pub fn warning(action: &str, subject: &str) -> String {
    format!(
        "{} {} {}",
        WARNING.if_supports_color(Stream::Stdout, |s| s.yellow().to_string()),
        action.if_supports_color(Stream::Stdout, |s| s.bold().to_string()),
        subject,
    )
}

pub fn format_error<E: std::fmt::Display>(err: &E) -> String {
    format!(
        "{} {}",
        FAILURE.if_supports_color(Stream::Stderr, |s| s.red().to_string()),
        err
    )
}

/// Cyan-rendered identifier (peer ids, vault uuids, paths).
pub fn ident<S: AsRef<str>>(s: S) -> String {
    if is_plain() {
        s.as_ref().to_string()
    } else {
        s.as_ref()
            .if_supports_color(Stream::Stdout, |x| x.cyan().to_string())
            .to_string()
    }
}

/// Dimmed-rendered hash / hex blob.
pub fn dim<S: AsRef<str>>(s: S) -> String {
    if is_plain() {
        s.as_ref().to_string()
    } else {
        s.as_ref()
            .if_supports_color(Stream::Stdout, |x| x.dimmed().to_string())
            .to_string()
    }
}

/// Yellow-rendered numeric (heights, counts).
pub fn num<S: AsRef<str>>(s: S) -> String {
    if is_plain() {
        s.as_ref().to_string()
    } else {
        s.as_ref()
            .if_supports_color(Stream::Stdout, |x| x.yellow().to_string())
            .to_string()
    }
}

/// Build a borderless, terminal-width-aware table.
pub fn make_table() -> Table {
    let mut t = Table::new();
    t.load_preset(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic);
    t
}
