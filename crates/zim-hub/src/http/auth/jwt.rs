//! Verify self-signed Ed25519 JWTs from `zim` daemons.
//!
//! Wire format (compact JWS, `alg=EdDSA`):
//!
//! ```text
//! header  = {"alg":"EdDSA","typ":"JWT","kid":"<pubkey_hex>"}
//! payload = {"iss":"<pubkey_hex>","aud":"<hub_url>","iat":N,"exp":N+60}
//! signature = ed25519_sign(b64url(header) || '.' || b64url(payload))
//! jwt = b64url(header) || '.' || b64url(payload) || '.' || b64url(sig)
//! ```
//!
//! The daemon mints these on the fly with the same identity key it
//! enrolled at device-code time. Verification doesn't touch the
//! database — the caller is expected to follow up with a
//! `user_peers` lookup against the returned pubkey to decide
//! whether the JWT belongs to a real user.
//!
//! Why roll our own? The `jsonwebtoken` crate supports EdDSA, but
//! its `DecodingKey::from_ed_der` wants PKCS#8-wrapped public keys;
//! we already speak raw 32-byte ed25519 pubkeys everywhere else in
//! the codebase and don't want to introduce a parallel encoding.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::Deserialize;
use thiserror::Error;
use zim_crypto::PublicKey;

/// Number of seconds of clock skew we tolerate on either side.
const CLOCK_SKEW_SECS: i64 = 30;

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

/// What a successful verify returns. The pubkey is enough to look
/// up the owning user in `user_peers`; the caller is responsible
/// for that step.
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

    // Signature covers the raw ASCII bytes of "<header>.<payload>".
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2])?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| JwtError::Malformed("signature length"))?;
    pubkey
        .verify_bytes(signing_input.as_bytes(), &sig_arr)
        .map_err(|_| JwtError::BadSignature)?;

    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1])?;
    let payload: Payload = serde_json::from_slice(&payload_bytes)?;

    // Normalize both sides — hub config may carry a trailing slash
    // that the daemon doesn't bother to strip when signing.
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

    Ok(Verified { pubkey })
}

/// Parse the JWT header and return the `kid` without verifying
/// anything. Used to route between the pubkey-hex daemon path and
/// the thumbprint browser path.
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

/// Verify a JWT whose signing key is already known. Used for the
/// thumbprint path where the caller looked up the pubkey from the DB
/// before calling this.
pub fn verify_with_pubkey(
    token: &str,
    expected_aud: &str,
    pubkey: &PublicKey,
) -> Result<(), JwtError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::Malformed("expected three dot-separated parts"));
    }
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2])?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| JwtError::Malformed("signature length"))?;
    pubkey
        .verify_bytes(signing_input.as_bytes(), &sig_arr)
        .map_err(|_| JwtError::BadSignature)?;

    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1])?;
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
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use zim_crypto::PrivateKey;

    /// Mirror of `zim::hub_jwt::mint`. Re-implemented here to keep
    /// the test self-contained — if the on-the-wire format ever
    /// drifts, both sides will fail to roundtrip and the test will
    /// loudly say so.
    fn mint(secret: &PrivateKey, hub_url: &str, exp_offset: i64) -> String {
        let pubkey_hex = secret.public().to_hex();
        let now = chrono::Utc::now().timestamp();
        let header = serde_json::json!({ "alg": "EdDSA", "typ": "JWT", "kid": pubkey_hex.clone() });
        let payload = serde_json::json!({
            "iss": pubkey_hex,
            "aud": hub_url.trim_end_matches('/'),
            "iat": now,
            "exp": now + exp_offset,
        });
        let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let signing_input = format!("{h}.{p}");
        let s = URL_SAFE_NO_PAD.encode(secret.sign(signing_input.as_bytes()).to_bytes());
        format!("{signing_input}.{s}")
    }

    #[test]
    fn roundtrip_succeeds_for_fresh_token() {
        let secret = PrivateKey::generate();
        let token = mint(&secret, "https://hub.example.com", 60);
        let v = verify(&token, "https://hub.example.com").expect("verify");
        assert_eq!(v.pubkey.to_hex(), secret.public().to_hex());
    }

    #[test]
    fn audience_mismatch_is_rejected() {
        let secret = PrivateKey::generate();
        let token = mint(&secret, "https://hub.example.com", 60);
        let err = verify(&token, "https://other.example.com").unwrap_err();
        assert!(matches!(err, JwtError::BadAudience(_)));
    }

    #[test]
    fn trailing_slash_in_audience_is_normalized() {
        let secret = PrivateKey::generate();
        // Daemon signs with no trailing slash; hub configured with
        // a trailing slash. Should still verify.
        let token = mint(&secret, "https://hub.example.com", 60);
        verify(&token, "https://hub.example.com/").expect("trailing slash");
    }

    #[test]
    fn expired_token_is_rejected() {
        let secret = PrivateKey::generate();
        // exp_offset well past the clock-skew tolerance window.
        let token = mint(&secret, "https://hub.example.com", -1000);
        let err = verify(&token, "https://hub.example.com").unwrap_err();
        assert!(matches!(err, JwtError::Expired));
    }

    #[test]
    fn signature_forgery_is_rejected() {
        let signer = PrivateKey::generate();
        let attacker_kid = PrivateKey::generate().public().to_hex();
        // Build a token that claims `attacker_kid` but is signed
        // with `signer`. The kid pubkey can't verify the
        // attacker's-key-claimed signature.
        let now = chrono::Utc::now().timestamp();
        let header = serde_json::json!({ "alg": "EdDSA", "typ": "JWT", "kid": attacker_kid });
        let payload = serde_json::json!({
            "iss": "anything",
            "aud": "https://hub.example.com",
            "iat": now,
            "exp": now + 60,
        });
        let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
        let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let signing_input = format!("{h}.{p}");
        let s = URL_SAFE_NO_PAD.encode(signer.sign(signing_input.as_bytes()).to_bytes());
        let token = format!("{signing_input}.{s}");

        let err = verify(&token, "https://hub.example.com").unwrap_err();
        assert!(matches!(err, JwtError::BadSignature));
    }
}
