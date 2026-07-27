//! Status lines and error rendering.
//!
//! These *return* strings — the op pattern formats in Display impls
//! and prints once, centrally (`main.rs`). Only [`format_error`] is
//! aimed at stderr.

use owo_colors::{OwoColorize, Stream};

use super::is_plain;

pub const SUCCESS: &str = "\u{2713}"; // ✓
pub const PROGRESS: &str = "\u{2192}"; // →
pub const FAILURE: &str = "\u{2717}"; // ✗
pub const WARNING: &str = "!";

fn status_line(sym: &str, sym_color: fn(&str) -> String, action: &str, subject: &str) -> String {
    if is_plain() {
        return format!("{action} {subject}").trim_end().to_string();
    }
    format!(
        "{} {} {}",
        sym_color(sym),
        action.if_supports_color(Stream::Stdout, |s| s.bold().to_string()),
        subject,
    )
    .trim_end()
    .to_string()
}

/// `✓ action subject` — the subject rendered as an identifier.
pub fn success(action: &str, subject: &str) -> String {
    let subject = super::ident(subject);
    status_line(
        SUCCESS,
        |s| {
            s.if_supports_color(Stream::Stdout, |s| s.green().to_string())
                .to_string()
        },
        action,
        &subject,
    )
}

/// `→ action subject` — something in flight.
pub fn progress(action: &str, subject: &str) -> String {
    status_line(
        PROGRESS,
        |s| {
            s.if_supports_color(Stream::Stdout, |s| s.cyan().to_string())
                .to_string()
        },
        action,
        subject,
    )
}

/// `✗ action subject`.
pub fn failure(action: &str, subject: &str) -> String {
    status_line(
        FAILURE,
        |s| {
            s.if_supports_color(Stream::Stdout, |s| s.red().to_string())
                .to_string()
        },
        action,
        subject,
    )
}

/// `! action subject`.
pub fn warning(action: &str, subject: &str) -> String {
    status_line(
        WARNING,
        |s| {
            s.if_supports_color(Stream::Stdout, |s| s.yellow().to_string())
                .to_string()
        },
        action,
        subject,
    )
}

/// Bold section header.
pub fn header<S: AsRef<str>>(s: S) -> String {
    super::bold(s)
}

/// `label: value` with the label dimmed — key/value detail lines.
pub fn label(label: &str, value: &str) -> String {
    format!("{} {value}", super::dim(format!("{label}:")))
}

/// `yes` / `no`, colored by polarity.
pub fn yes_no(v: bool) -> String {
    if v {
        super::good("yes")
    } else {
        super::bad("no")
    }
}

/// `✗ error` plus a `caused by:` line per source in the chain —
/// stderr-aimed, used once in `main.rs`.
pub fn format_error(err: &(dyn std::error::Error + 'static)) -> String {
    use std::fmt::Write;
    let mut out = format!(
        "{} {err}",
        FAILURE.if_supports_color(Stream::Stderr, |s| s.red().to_string()),
    );
    let mut source = err.source();
    while let Some(cause) = source {
        let _ = write!(
            out,
            "\n  {} {cause}",
            "caused by:".if_supports_color(Stream::Stderr, |s| s.yellow().to_string())
        );
        source = cause.source();
    }
    out
}
