//! The app's half of the sign-in handoff (the hub's half is
//! `zim-hub`'s `http/auth/mobile.rs`): a PKCE-style verifier the app
//! never lets out, the sheet URL that registers its hash, and reading
//! the callback the hub redirects the sheet to.
//!
//! Another app can claim `zim-auth://` and read the token off the
//! callback. It can't redeem it: the hub also wants the verifier.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};
use url::Url;

/// Scheme the hub redirects the sheet to; registered in `Info.ios.plist`.
pub const CALLBACK_SCHEME: &str = "zim-auth";

/// A fresh verifier: 32 random bytes, base64url (43 chars).
pub fn new_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// What the hub is told at sign-in start: base64url SHA-256 of `verifier`.
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The URL for the sheet: the provider's start route on `hub`, carrying
/// the page's `next` and the challenge.
pub fn sheet_url(hub: &Url, path: &str, next: Option<&str>, verifier: &str) -> Url {
    let mut url = hub.join(path).expect("start path joins onto the hub");
    {
        let mut q = url.query_pairs_mut();
        if let Some(next) = next {
            q.append_pair("next", next);
        }
        q.append_pair("mobile_challenge", &challenge(verifier));
    }
    url
}

/// What the sheet's final redirect said.
#[derive(Debug, PartialEq, Eq)]
pub enum Callback {
    /// Sign-in succeeded: redeem this token.
    Token(String),
    /// The hub refused or the provider failed.
    Failed,
}

/// Read the callback URL; anything but `zim-auth://callback?token=…` is a failure.
pub fn parse_callback(raw: &str) -> Callback {
    let Ok(url) = Url::parse(raw) else {
        return Callback::Failed;
    };
    if url.scheme() != CALLBACK_SCHEME || url.host_str() != Some("callback") {
        return Callback::Failed;
    }
    match url.query_pairs().find(|(k, _)| k == "token") {
        Some((_, token)) if !token.is_empty() => Callback::Token(token.into_owned()),
        _ => Callback::Failed,
    }
}

/// Where the webview redeems `token`: sets the session cookie in its own
/// cookie store, then lands on the SPA.
pub fn complete_url(hub: &Url, token: &str, verifier: &str) -> Url {
    let mut url = hub
        .join("/auth/mobile/complete")
        .expect("joins onto the hub");
    url.query_pairs_mut()
        .append_pair("token", token)
        .append_pair("verifier", verifier);
    url
}

/// Where the webview lands when sign-in failed.
pub fn failed_url(hub: &Url) -> Url {
    let mut url = hub.join("/auth/login").expect("joins onto the hub");
    url.query_pairs_mut().append_pair("error", "failed");
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub() -> Url {
        Url::parse("https://hub.example.org").unwrap()
    }

    #[test]
    fn the_challenge_is_the_rfc_7636_s256_of_the_verifier() {
        // RFC 7636 appendix B — also what the hub's tests use.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn every_verifier_is_fresh_and_hub_sized() {
        let (a, b) = (new_verifier(), new_verifier());
        assert_ne!(a, b);
        assert_eq!(a.len(), 43);
        assert_eq!(challenge(&a).len(), 43);
    }

    #[test]
    fn the_sheet_gets_the_challenge_but_never_the_verifier() {
        let url = sheet_url(
            &hub(),
            "/auth/google/login",
            Some("/v/abc"),
            "secret-verifier",
        );
        let query = url.query().unwrap();
        assert_eq!(url.path(), "/auth/google/login");
        assert!(query.contains("next=%2Fv%2Fabc"));
        assert!(query.contains(&format!(
            "mobile_challenge={}",
            challenge("secret-verifier")
        )));
        assert!(!url.as_str().contains("secret-verifier"));
    }

    #[test]
    fn alices_callback_carries_a_token() {
        assert_eq!(
            parse_callback("zim-auth://callback?token=abc_123-x"),
            Callback::Token("abc_123-x".into())
        );
    }

    #[test]
    fn a_callback_that_is_not_ours_is_a_failure() {
        for raw in [
            "zim-auth://callback?error=failed",
            "zim-auth://callback?token=",
            "zim-auth://elsewhere?token=t",
            "https://hub.example.org/?token=t",
            "not a url",
        ] {
            assert_eq!(parse_callback(raw), Callback::Failed, "{raw}");
        }
    }

    #[test]
    fn the_webview_redeems_with_token_and_verifier() {
        let url = complete_url(&hub(), "tok", "ver");
        assert_eq!(
            url.as_str(),
            "https://hub.example.org/auth/mobile/complete?token=tok&verifier=ver"
        );
    }
}
