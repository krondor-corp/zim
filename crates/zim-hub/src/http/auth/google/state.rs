//! OAuth `state`: carries `next` through Google's round trip and binds
//! the callback to the browser that started it.
//!
//! `state = <nonce>.<next>`. The nonce is also set in a short-lived
//! `oauth_state` cookie scoped to `/auth/google`; the callback only
//! proceeds when the two match, so a callback URL crafted elsewhere
//! (login CSRF — signing Alice into Mallory's account) is rejected.
//! `next` is re-sanitized on the way back.

use axum::http::HeaderMap;
use rand::Rng;

use crate::http::auth::{cookie, return_to};

const COOKIE: &str = "oauth_state";
const MAX_AGE_SECS: u32 = 600;

/// Start a handshake: the `state` param for Google, and the cookie
/// that must come back with it.
pub fn begin(next: &str, secure: bool) -> (String, String) {
    let nonce: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let state = format!("{nonce}.{}", return_to::sanitize(Some(next)));
    (state, set_cookie(&nonce, MAX_AGE_SECS, secure))
}

/// Check a callback's `state` against the cookie; the `next` to land
/// on, or `None` when they don't match.
pub fn finish(state: &str, headers: &HeaderMap) -> Option<String> {
    let (nonce, next) = state.split_once('.')?;
    let expected = cookie(headers, COOKIE)?;
    if nonce.is_empty() || nonce != expected {
        return None;
    }
    Some(return_to::sanitize(Some(next)).to_string())
}

/// Expire the cookie once the handshake is done.
pub fn clear_cookie(secure: bool) -> String {
    set_cookie("", 0, secure)
}

fn set_cookie(value: &str, max_age: u32, secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "{COOKIE}={value}; Path=/auth/google; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure_attr}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::COOKIE as COOKIE_HEADER;

    fn browser_with(cookie: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(COOKIE_HEADER, cookie.parse().unwrap());
        h
    }

    fn nonce_cookie(set_cookie: &str) -> String {
        set_cookie.split(';').next().unwrap().to_string()
    }

    #[test]
    fn alice_returns_to_where_she_started() {
        let (state, set_cookie) = begin("/device?code=XYZ", false);
        let alice = browser_with(&nonce_cookie(&set_cookie));
        assert_eq!(finish(&state, &alice).as_deref(), Some("/device?code=XYZ"));
    }

    #[test]
    fn a_callback_without_alices_cookie_is_rejected() {
        // Mallory started the handshake and sends Alice the callback URL.
        let (mallorys_state, _) = begin("/", false);
        let (_, alices_cookie) = begin("/", false);
        let alice = browser_with(&nonce_cookie(&alices_cookie));
        assert_eq!(finish(&mallorys_state, &alice), None);
        assert_eq!(finish(&mallorys_state, &HeaderMap::new()), None);
    }

    #[test]
    fn a_tampered_next_still_cannot_leave_the_hub() {
        let (state, set_cookie) = begin("/", false);
        let nonce = state.split_once('.').unwrap().0;
        let tampered = format!("{nonce}.//evil.com");
        let alice = browser_with(&nonce_cookie(&set_cookie));
        assert_eq!(finish(&tampered, &alice).as_deref(), Some("/"));
    }

    #[test]
    fn malformed_state_is_rejected() {
        let alice = browser_with("oauth_state=abc");
        assert_eq!(finish("no-separator", &alice), None);
        assert_eq!(finish(".", &browser_with("oauth_state=")), None);
    }
}
