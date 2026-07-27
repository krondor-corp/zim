//! Self-signed Ed25519 JWTs — mint *and* verify, in one place.
//!
//! Both ends of the format live in this module so they can't drift and
//! the roundtrip is unit-tested against the real implementations. The
//! daemon (and any client) [`mint`]s a fresh, short-lived token per
//! request; the hub server [`verify`]s it. Nothing here is hub-specific
//! beyond convention: the audience (`aud`) is just a string both sides
//! agree on — for hub auth it's the hub URL.
//!
//! Wire format (compact JWS, `alg=EdDSA`):
//!
//! ```text
//! header  = {"alg":"EdDSA","typ":"JWT","kid":"<pubkey_hex>"}
//! payload = {"iss":"<pubkey_hex>","aud":"<aud>","iat":N,"exp":N+60}
//! sig     = ed25519_sign(b64url(header) || '.' || b64url(payload))
//! jwt     = b64url(header) || '.' || b64url(payload) || '.' || b64url(sig)
//! ```
//!
//! No long-lived bearer token is held — the only secret is the signing
//! key (for a daemon, the `identity.key` it enrolled at device-code
//! time).
//!
//! Why roll our own? The `jsonwebtoken` crate supports EdDSA, but its
//! `DecodingKey::from_ed_der` wants PKCS#8-wrapped public keys; we
//! already speak raw 32-byte ed25519 pubkeys everywhere else and don't
//! want a parallel encoding.
//!
//! (The browser wasm SDK mints the same format inline — it needs
//! `js_sys::Date` for time, where `chrono::Utc::now` can't run — so the
//! tests here are the reference its output must match.)

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;
use thiserror::Error;
use zim_crypto::{PrivateKey, PublicKey};

/// Lifetime of a minted token. Short on purpose — keeps the replay
/// window tight without forcing precise clock sync (verification
/// tolerates ±[`CLOCK_SKEW_SECS`], leaving ~30s of usable validity at
/// worst-case drift).
const TTL_SECS: i64 = 60;

/// Seconds of clock skew tolerated on either side when verifying.
const CLOCK_SKEW_SECS: i64 = 30;

// ─── Mint ──────────────────────────────────────────────────────────────

/// Mint a JWT for `aud`, signed by `secret`.
///
/// `aud` is normalized (trailing slash stripped) so callers can pass a
/// URL through verbatim — e.g. `hub-session.json`'s `hub_url` — without
/// a trailing-slash mismatch against the verifier's configured value.
pub fn mint(secret: &PrivateKey, aud: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    mint_at(secret, aud, now, now + TTL_SECS)
}

/// [`mint`] with explicit `iat`/`exp` — the shared body, also used by
/// tests to build expired / not-yet-valid tokens with the real minter.
fn mint_at(secret: &PrivateKey, aud: &str, iat: i64, exp: i64) -> String {
    let pubkey_hex = secret.public().to_hex();

    let header = serde_json::json!({
        "alg": "EdDSA",
        "typ": "JWT",
        "kid": pubkey_hex,
    });
    let payload = serde_json::json!({
        "iss": pubkey_hex,
        "aud": aud.trim_end_matches('/'),
        "iat": iat,
        "exp": exp,
    });

    let header_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).expect("header is json-serializable"));
    let payload_b64 =
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("payload is json-serializable"));

    let signing_input = format!("{header_b64}.{payload_b64}");
    let signature = secret.sign(signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    format!("{signing_input}.{sig_b64}")
}

// ─── Verify ────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("malformed: {0}")]
    Malformed(&'static str),
    #[error("base64 decode: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported alg: {0}")]
    UnsupportedAlg(String),
    #[error("kid not a valid pubkey hex")]
    BadKid,
    #[error("signature does not verify")]
    BadSignature,
    #[error("expired")]
    Expired,
    #[error("issued in the future")]
    NotYet,
    #[error("audience mismatch: token says {0}")]
    BadAudience(String),
}

/// What a successful [`verify`] returns. The pubkey identifies the
/// signer; whether that key belongs to a real user is the caller's
/// follow-up (the hub checks `user_peers`).
#[derive(Debug)]
pub struct Verified {
    pub pubkey: PublicKey,
}

/// The `kid` claim from the JWT header, used to route the lookup.
pub enum Kid {
    /// 64-char hex pubkey — daemon-enrolled path, lookup by pubkey.
    Pubkey(PublicKey),
    /// RFC 7638 JWK thumbprint — browser path, lookup by thumbprint.
    Thumbprint(String),
}

#[derive(Debug, Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Payload {
    aud: String,
    iat: i64,
    exp: i64,
}

/// Verify `token` against `expected_aud`: signature (by the header's
/// `kid` pubkey), audience (trailing-slash-normalized on both sides),
/// and freshness (±[`CLOCK_SKEW_SECS`]).
pub fn verify(token: &str, expected_aud: &str) -> Result<Verified, JwtError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::Malformed("expected three dot-separated parts"));
    }

    let header_bytes = URL_SAFE_NO_PAD.decode(parts[0])?;
    let header: Header = serde_json::from_slice(&header_bytes)?;
    if header.alg != "EdDSA" {
        return Err(JwtError::UnsupportedAlg(header.alg));
    }
    let kid = header.kid.ok_or(JwtError::Malformed("missing kid"))?;
    let pubkey = PublicKey::from_hex(&kid).map_err(|_| JwtError::BadKid)?;

    check_signature(&parts, &pubkey)?;
    check_claims(parts[1], expected_aud)?;
    Ok(Verified { pubkey })
}

/// Verify a JWT whose signing key is already known. Used for the
/// thumbprint path where the caller looked up the pubkey from its
/// device table before calling this.
pub fn verify_with_pubkey(
    token: &str,
    expected_aud: &str,
    pubkey: &PublicKey,
) -> Result<(), JwtError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::Malformed("expected three dot-separated parts"));
    }
    check_signature(&parts, pubkey)?;
    check_claims(parts[1], expected_aud)
}

/// Parse the JWT header and return the `kid` without verifying
/// anything. Used to route between the pubkey-hex daemon path and the
/// thumbprint browser path.
pub fn peek_kid(token: &str) -> Result<Kid, JwtError> {
    let first = token
        .split('.')
        .next()
        .ok_or(JwtError::Malformed("expected three parts"))?;
    let header_bytes = URL_SAFE_NO_PAD.decode(first)?;
    let header: Header = serde_json::from_slice(&header_bytes)?;
    let kid = header.kid.ok_or(JwtError::Malformed("missing kid"))?;
    // 64 hex chars = 32-byte pubkey.
    if kid.len() == 64 && kid.chars().all(|c| c.is_ascii_hexdigit()) {
        let pk = PublicKey::from_hex(&kid).map_err(|_| JwtError::BadKid)?;
        Ok(Kid::Pubkey(pk))
    } else {
        Ok(Kid::Thumbprint(kid))
    }
}

/// Signature covers the raw ASCII bytes of `"<header>.<payload>"`.
fn check_signature(parts: &[&str], pubkey: &PublicKey) -> Result<(), JwtError> {
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2])?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| JwtError::Malformed("signature length"))?;
    pubkey
        .verify_bytes(signing_input.as_bytes(), &sig_arr)
        .map_err(|_| JwtError::BadSignature)
}

/// Audience (trailing-slash-normalized) + freshness window.
fn check_claims(payload_b64: &str, expected_aud: &str) -> Result<(), JwtError> {
    let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64)?;
    let payload: Payload = serde_json::from_slice(&payload_bytes)?;

    if payload.aud.trim_end_matches('/') != expected_aud.trim_end_matches('/') {
        return Err(JwtError::BadAudience(payload.aud));
    }

    let now = chrono::Utc::now().timestamp();
    if now + CLOCK_SKEW_SECS < payload.iat {
        return Err(JwtError::NotYet);
    }
    if now - CLOCK_SKEW_SECS > payload.exp {
        return Err(JwtError::Expired);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both ends are the REAL implementations — no hand-mirrored mint.
    // If either side of the format drifts, these fail loudly.

    #[test]
    fn mint_verify_roundtrip_succeeds() {
        let alice = PrivateKey::generate();
        let token = mint(&alice, "https://hub.example.com");
        let v = verify(&token, "https://hub.example.com").expect("fresh token verifies");
        assert_eq!(v.pubkey.to_hex(), alice.public().to_hex());
    }

    #[test]
    fn audience_mismatch_is_rejected() {
        let alice = PrivateKey::generate();
        let token = mint(&alice, "https://hub.example.com");
        let err = verify(&token, "https://other.example.com").unwrap_err();
        assert!(matches!(err, JwtError::BadAudience(_)));
    }

    #[test]
    fn trailing_slashes_normalize_on_both_ends() {
        let alice = PrivateKey::generate();
        // Minted WITH a slash, verified WITHOUT — and vice versa.
        let token = mint(&alice, "https://hub.example.com/");
        verify(&token, "https://hub.example.com").expect("mint-side slash");
        let token = mint(&alice, "https://hub.example.com");
        verify(&token, "https://hub.example.com/").expect("verify-side slash");
    }

    #[test]
    fn expired_token_is_rejected() {
        let alice = PrivateKey::generate();
        let now = chrono::Utc::now().timestamp();
        // Expired well past the skew tolerance.
        let token = mint_at(&alice, "https://hub.example.com", now - 2000, now - 1000);
        let err = verify(&token, "https://hub.example.com").unwrap_err();
        assert!(matches!(err, JwtError::Expired));
    }

    #[test]
    fn token_issued_in_the_future_is_rejected() {
        let alice = PrivateKey::generate();
        let now = chrono::Utc::now().timestamp();
        let token = mint_at(&alice, "https://hub.example.com", now + 1000, now + 2000);
        let err = verify(&token, "https://hub.example.com").unwrap_err();
        assert!(matches!(err, JwtError::NotYet));
    }

    #[test]
    fn signature_forgery_is_rejected() {
        // Mallory signs with her key but claims Alice's kid: the kid
        // pubkey can't verify Mallory's signature.
        let alice = PrivateKey::generate();
        let mallory = PrivateKey::generate();
        let token = mint(&mallory, "https://hub.example.com");
        let honest = mint(&alice, "https://hub.example.com");
        // Splice Alice's header (her kid) onto Mallory's payload+sig.
        let forged = format!(
            "{}.{}",
            honest.split('.').next().unwrap(),
            token.split('.').skip(1).collect::<Vec<_>>().join(".")
        );
        let err = verify(&forged, "https://hub.example.com").unwrap_err();
        assert!(matches!(err, JwtError::BadSignature));
    }

    #[test]
    fn tampered_payload_is_rejected() {
        let alice = PrivateKey::generate();
        let token = mint(&alice, "https://hub.example.com");
        let mut parts: Vec<String> = token.split('.').map(str::to_string).collect();
        // Re-aim the audience without re-signing.
        parts[1] = URL_SAFE_NO_PAD.encode(
            serde_json::json!({
                "iss": alice.public().to_hex(),
                "aud": "https://evil.example.com",
                "iat": chrono::Utc::now().timestamp(),
                "exp": chrono::Utc::now().timestamp() + 60,
            })
            .to_string(),
        );
        let err = verify(&parts.join("."), "https://evil.example.com").unwrap_err();
        assert!(matches!(err, JwtError::BadSignature));
    }

    #[test]
    fn malformed_tokens_are_rejected_not_panicked_on() {
        for garbage in ["", "a.b", "a.b.c.d", "!!!.???.###"] {
            assert!(verify(garbage, "aud").is_err(), "{garbage:?} must error");
        }
    }

    #[test]
    fn verify_with_pubkey_checks_the_supplied_key() {
        let alice = PrivateKey::generate();
        let bob = PrivateKey::generate();
        let token = mint(&alice, "https://hub.example.com");
        verify_with_pubkey(&token, "https://hub.example.com", &alice.public())
            .expect("signer's key verifies");
        assert!(matches!(
            verify_with_pubkey(&token, "https://hub.example.com", &bob.public()),
            Err(JwtError::BadSignature)
        ));
    }

    #[test]
    fn peek_kid_routes_pubkey_vs_thumbprint() {
        let alice = PrivateKey::generate();
        let token = mint(&alice, "aud");
        assert!(matches!(peek_kid(&token).unwrap(), Kid::Pubkey(pk)
            if pk == alice.public()));

        // A non-hex kid (browser thumbprint path) routes as Thumbprint.
        let header = URL_SAFE_NO_PAD
            .encode(serde_json::json!({"alg": "EdDSA", "kid": "not-a-pubkey"}).to_string());
        assert!(matches!(
            peek_kid(&format!("{header}.x.y")).unwrap(),
            Kid::Thumbprint(t) if t == "not-a-pubkey"
        ));
    }
}
