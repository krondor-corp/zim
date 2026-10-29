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

use crate::http::auth::{cookie, mobile, return_to};

const COOKIE: &str = "oauth_state";
/// Set only for the native app's sign-in: the PKCE challenge to issue
/// the handoff against. Held in a cookie (not the `state` param) so it
/// is bound to the browser that started the handshake, like the nonce.
const MOBILE_COOKIE: &str = "oauth_mobile";
const MAX_AGE_SECS: u32 = 600;

/// A callback that matches the browser that started it.
#[derive(Debug, PartialEq)]
pub struct Finished {
    /// Where to land.
    pub next: String,
    /// The app's challenge, when this handshake belongs to the native app.
    pub mobile_challenge: Option<String>,
}

/// Start a handshake: the `state` param for Google, and the cookies
/// that must come back with it.
pub fn begin(next: &str, mobile_challenge: Option<&str>, secure: bool) -> (String, Vec<String>) {
    let nonce: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let state = format!("{nonce}.{}", return_to::sanitize(Some(next)));
    let mut cookies = vec![set_cookie(COOKIE, &nonce, MAX_AGE_SECS, secure)];
    if let Some(challenge) = mobile_challenge {
        cookies.push(set_cookie(MOBILE_COOKIE, challenge, MAX_AGE_SECS, secure));
    }
    (state, cookies)
}

/// Check a callback's `state` against the cookie; where to land, or
/// `None` when they don't match.
pub fn finish(state: &str, headers: &HeaderMap) -> Option<Finished> {
    let (nonce, next) = state.split_once('.')?;
    let expected = cookie(headers, COOKIE)?;
    if nonce.is_empty() || nonce != expected {
        return None;
    }
    Some(Finished {
        next: return_to::sanitize(Some(next)).to_string(),
        mobile_challenge: cookie(headers, MOBILE_COOKIE).filter(|c| mobile::valid_challenge(c)),
    })
}

/// Expire the cookies once the handshake is done.
pub fn clear_cookies(secure: bool) -> [String; 2] {
    [
        set_cookie(COOKIE, "", 0, secure),
        set_cookie(MOBILE_COOKIE, "", 0, secure),
    ]
}

fn set_cookie(name: &str, value: &str, max_age: u32, secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "{name}={value}; Path=/auth/google; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure_attr}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::COOKIE as COOKIE_HEADER;

    const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

    fn browser_with(cookies: &[String]) -> HeaderMap {
        let pairs: Vec<&str> = cookies
            .iter()
            .map(|c| c.split(';').next().unwrap())
            .collect();
        let mut h = HeaderMap::new();
        h.insert(COOKIE_HEADER, pairs.join("; ").parse().unwrap());
        h
    }

    #[test]
    fn alice_returns_to_where_she_started() {
        let (state, cookies) = begin("/auth/device?code=XYZ", None, false);
        let alice = browser_with(&cookies);
        assert_eq!(
            finish(&state, &alice),
            Some(Finished {
                next: "/auth/device?code=XYZ".into(),
                mobile_challenge: None,
            })
        );
    }

    #[test]
    fn alices_phone_gets_its_challenge_back_after_the_round_trip() {
        let (state, cookies) = begin("/", Some(CHALLENGE), false);
        let phone = browser_with(&cookies);
        let finished = finish(&state, &phone).unwrap();
        assert_eq!(finished.mobile_challenge.as_deref(), Some(CHALLENGE));
    }

    #[test]
    fn a_callback_without_alices_cookie_is_rejected() {
        // Mallory started the handshake and sends Alice the callback URL.
        let (mallorys_state, _) = begin("/", None, false);
        let (_, alices_cookies) = begin("/", None, false);
        let alice = browser_with(&alices_cookies);
        assert_eq!(finish(&mallorys_state, &alice), None);
        assert_eq!(finish(&mallorys_state, &HeaderMap::new()), None);
    }

    #[test]
    fn a_tampered_next_still_cannot_leave_the_hub() {
        let (state, cookies) = begin("/", None, false);
        let nonce = state.split_once('.').unwrap().0;
        let tampered = format!("{nonce}.//evil.com");
        let alice = browser_with(&cookies);
        assert_eq!(finish(&tampered, &alice).unwrap().next, "/");
    }

    #[test]
    fn malformed_state_is_rejected() {
        let alice = browser_with(&["oauth_state=abc".into()]);
        assert_eq!(finish("no-separator", &alice), None);
        assert_eq!(finish(".", &browser_with(&["oauth_state=".into()])), None);
    }

    #[test]
    fn a_forged_challenge_cookie_is_ignored() {
        let (state, mut cookies) = begin("/", None, false);
        cookies.push("oauth_mobile=short; Path=/".into());
        let finished = finish(&state, &browser_with(&cookies)).unwrap();
        assert_eq!(finished.mobile_challenge, None);
    }
}
