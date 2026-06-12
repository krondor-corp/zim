//! Self-signed Ed25519 JWTs for daemon → hub calls.
//!
//! Hub verifier lives at `zim_hub::http::auth::jwt`. This module is
//! the daemon side: mint a fresh, short-lived JWT for each request.
//!
//! Wire format (compact JWS, `alg=EdDSA`):
//!
//! ```text
//! header  = {"alg":"EdDSA","typ":"JWT","kid":"<pubkey_hex>"}
//! payload = {"iss":"<pubkey_hex>","aud":"<hub_url>","iat":N,"exp":N+60}
//! sig     = ed25519_sign(b64url(header) || '.' || b64url(payload))
//! jwt     = b64url(header) || '.' || b64url(payload) || '.' || b64url(sig)
//! ```
//!
//! No long-lived bearer token is held on the daemon — the only
//! secret on disk is `identity.key`, the same one the daemon
//! enrolled at device-code time.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use zim_crypto::PrivateKey;

/// Lifetime of a minted token. Short on purpose — keeps the replay
/// window tight without forcing precise clock sync (the hub
/// tolerates ±30s skew, leaving ~30s of usable validity at the
/// worst-case clock drift).
const TTL_SECS: i64 = 60;

/// Mint a JWT for `hub_url`, signed by `secret`.
///
/// `hub_url` is normalized (trailing slash stripped) so the daemon
/// can pass `hub-session.json`'s `hub_url` field through verbatim
/// without worrying about a trailing-slash mismatch with what the
/// hub has configured as its `HOST_NAME`.
pub fn mint(secret: &PrivateKey, hub_url: &str) -> String {
    let pubkey_hex = secret.public().to_hex();
    let now = chrono::Utc::now().timestamp();

    let header = serde_json::json!({
        "alg": "EdDSA",
        "typ": "JWT",
        "kid": pubkey_hex,
    });
    let payload = serde_json::json!({
        "iss": pubkey_hex,
        "aud": hub_url.trim_end_matches('/'),
        "iat": now,
        "exp": now + TTL_SECS,
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
