//! Semantic text colorers. Each names *what the text is*, not what
//! color it gets — Display impls say `ident(vault_id)` and the palette
//! stays swappable in one place. All of them no-op under `--plain`
//! and when the stream isn't a color-capable tty.

use owo_colors::{OwoColorize, Stream};

use super::is_plain;

fn styled<S: AsRef<str>>(s: S, f: impl Fn(&str) -> String) -> String {
    if is_plain() {
        s.as_ref().to_string()
    } else {
        f(s.as_ref())
    }
}

/// Cyan — identifiers: peer ids, vault ids/names, paths, nicks.
pub fn ident<S: AsRef<str>>(s: S) -> String {
    styled(s, |x| {
        x.if_supports_color(Stream::Stdout, |x| x.cyan().to_string())
            .to_string()
    })
}

/// Dimmed — secondary info: hashes, hex blobs, hints, parentheticals.
pub fn dim<S: AsRef<str>>(s: S) -> String {
    styled(s, |x| {
        x.if_supports_color(Stream::Stdout, |x| x.dimmed().to_string())
            .to_string()
    })
}

/// Yellow — numerics: heights, counts, sizes.
pub fn num<S: AsRef<str>>(s: S) -> String {
    styled(s, |x| {
        x.if_supports_color(Stream::Stdout, |x| x.yellow().to_string())
            .to_string()
    })
}

/// Bold — emphasis within a line.
pub fn bold<S: AsRef<str>>(s: S) -> String {
    styled(s, |x| {
        x.if_supports_color(Stream::Stdout, |x| x.bold().to_string())
            .to_string()
    })
}

/// Yellow — inline warning text (no `!` prefix; see `output::warning`).
pub fn warn_text<S: AsRef<str>>(s: S) -> String {
    styled(s, |x| {
        x.if_supports_color(Stream::Stdout, |x| x.yellow().to_string())
            .to_string()
    })
}

/// Green — affirmative/live states (`up`, `mounted`, `trusted`).
pub fn good<S: AsRef<str>>(s: S) -> String {
    styled(s, |x| {
        x.if_supports_color(Stream::Stdout, |x| x.green().to_string())
            .to_string()
    })
}

/// Red — negative/dead states (`down`, `error`, `revoked`).
pub fn bad<S: AsRef<str>>(s: S) -> String {
    styled(s, |x| {
        x.if_supports_color(Stream::Stdout, |x| x.red().to_string())
            .to_string()
    })
}
