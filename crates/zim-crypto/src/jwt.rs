//! Compact JWT mint + verify for self-sovereign authentication.
//!
//! Format: JWS Compact Serialization (RFC 7515) with `alg: EdDSA`
//! (RFC 8037). Three base64url segments separated by `.`:
//!
//! ```text
//! base64url(header) . base64url(payload) . base64url(signature)
//! ```
//!
//! Header: `{"alg":"EdDSA","typ":"JWT","kid":"<thumbprint>"}`
//! Payload: `{"sub":"<thumbprint>","aud":"<aud>","iat":<unix>,"exp":<unix>}`
//!
//! `sub` and `kid` are both the RFC 7638 JWK thumbprint of the signing
//! key — SHA-256 of the canonical OKP JWK JSON, base64url-encoded.
//! The verifier looks the thumbprint up in its device table to get the
//! public key, then verifies the signature.
//!
//! [`PublicKey::jwk_thumbprint`] computes the thumbprint; it's also the
//! canonical device fingerprint used everywhere in Zim.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use sha2::{Digest, Sha256};

use crate::keys::{PrivateKey, PublicKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JwtClaims {
    /// RFC 7638 JWK thumbprint of the signing key.
    pub sub: String,
    /// Intended audience — the hub's `did:web` DID.
    pub aud: String,
    /// Issued-at (Unix seconds).
    pub iat: u64,
    /// Expiry (Unix seconds).
    pub exp: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
    #[error("malformed token: {0}")]
    Malformed(String),
    #[error("invalid signature")]
    InvalidSignature,
    #[error("wrong audience: got {got}, want {want}")]
    WrongAudience { got: String, want: String },
    #[error("token expired")]
    Expired,
    #[error("token not yet valid")]
    NotYetValid,
}

impl PublicKey {
    /// RFC 7638 JWK thumbprint for this Ed25519 key.
    ///
    /// SHA-256 of the canonical OKP JWK JSON — members in
    /// lexicographic order, no whitespace:
    /// `{"crv":"Ed25519","kty":"OKP","x":"<base64url-pubkey>"}`.
    /// The result is base64url-encoded (no padding).
    ///
    /// This is the canonical device fingerprint in Zim: stored in
    /// `user_peers`, used as JWT `sub` and `kid`.
    pub fn jwk_thumbprint(&self) -> String {
        let x = URL_SAFE_NO_PAD.encode(self.to_bytes());
        // Members in lexicographic order per RFC 7638 §3.3.
        let jwk = format!(r#"{{"crv":"Ed25519","kty":"OKP","x":"{x}"}}"#);
        let digest = Sha256::digest(jwk.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    }

    /// Verify a compact JWT. Checks signature, `aud`, and expiry.
    /// The caller supplies `now_secs` (Unix seconds) so this is
    /// testable without touching the system clock.
    pub fn verify_jwt(
        &self,
        token: &str,
        expected_aud: &str,
        now_secs: u64,
    ) -> Result<JwtClaims, JwtError> {
        let parts: Vec<&str> = token.splitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(JwtError::Malformed("expected 3 parts".into()));
        }
        let signing_input = &token[..parts[0].len() + 1 + parts[1].len()];

        let sig_bytes = URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|e| JwtError::Malformed(format!("sig base64: {e}")))?;
        if sig_bytes.len() != 64 {
            return Err(JwtError::Malformed("signature must be 64 bytes".into()));
        }
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        self.verify_bytes(signing_input.as_bytes(), &sig_arr)
            .map_err(|_| JwtError::InvalidSignature)?;

        let payload_bytes = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|e| JwtError::Malformed(format!("payload base64: {e}")))?;
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
            .map_err(|e| JwtError::Malformed(format!("payload JSON: {e}")))?;

        let get_str = |key: &str| -> Result<String, JwtError> {
            payload[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| JwtError::Malformed(format!("missing `{key}`")))
        };
        let get_u64 = |key: &str| -> Result<u64, JwtError> {
            payload[key]
                .as_u64()
                .ok_or_else(|| JwtError::Malformed(format!("missing `{key}`")))
        };

        let sub = get_str("sub")?;
        let aud = get_str("aud")?;
        let iat = get_u64("iat")?;
        let exp = get_u64("exp")?;

        if aud != expected_aud {
            return Err(JwtError::WrongAudience {
                got: aud,
                want: expected_aud.to_owned(),
            });
        }
        if now_secs < iat {
            return Err(JwtError::NotYetValid);
        }
        if now_secs >= exp {
            return Err(JwtError::Expired);
        }

        Ok(JwtClaims { sub, aud, iat, exp })
    }
}

impl PrivateKey {
    /// Mint a compact JWT signed with this key.
    ///
    /// `sub` is whatever the caller wants to identify the principal
    /// (typically the JWK thumbprint of the signing key).
    /// `aud` is the hub's DID. `exp_secs` is the lifetime in seconds
    /// from `now_secs`.
    pub fn sign_jwt(&self, sub: &str, aud: &str, iat: u64, exp: u64) -> String {
        let kid = self.public().jwk_thumbprint();
        let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT", "kid": kid});
        let payload = serde_json::json!({"sub": sub, "aud": aud, "iat": iat, "exp": exp});

        let h = URL_SAFE_NO_PAD.encode(header.to_string());
        let p = URL_SAFE_NO_PAD.encode(payload.to_string());
        let signing_input = format!("{h}.{p}");
        let sig = self.sign(signing_input.as_bytes());
        let s = URL_SAFE_NO_PAD.encode(sig.to_bytes());
        format!("{signing_input}.{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000;
    const AUD: &str = "did:web:hub.example.com";

    fn alice_signs(sub: &str, iat: u64, exp: u64) -> (PrivateKey, String) {
        let key = PrivateKey::generate();
        let token = key.sign_jwt(sub, AUD, iat, exp);
        (key, token)
    }

    #[test]
    fn valid_token_roundtrips() {
        let sub = "alice-device-fingerprint";
        let (key, token) = alice_signs(sub, NOW, NOW + 300);
        let claims = key
            .public()
            .verify_jwt(&token, AUD, NOW + 1)
            .expect("valid token");
        assert_eq!(claims.sub, sub);
        assert_eq!(claims.aud, AUD);
        assert_eq!(claims.iat, NOW);
        assert_eq!(claims.exp, NOW + 300);
    }

    #[test]
    fn wrong_key_rejected() {
        let (_, token) = alice_signs("sub", NOW, NOW + 300);
        let other = PrivateKey::generate();
        assert!(matches!(
            other.public().verify_jwt(&token, AUD, NOW + 1),
            Err(JwtError::InvalidSignature)
        ));
    }

    #[test]
    fn expired_token_rejected() {
        let (key, token) = alice_signs("sub", NOW, NOW + 300);
        assert!(matches!(
            key.public().verify_jwt(&token, AUD, NOW + 300),
            Err(JwtError::Expired)
        ));
    }

    #[test]
    fn wrong_audience_rejected() {
        let (key, token) = alice_signs("sub", NOW, NOW + 300);
        assert!(matches!(
            key.public()
                .verify_jwt(&token, "did:web:other.example.com", NOW + 1),
            Err(JwtError::WrongAudience { .. })
        ));
    }

    #[test]
    fn not_yet_valid_rejected() {
        let (key, token) = alice_signs("sub", NOW + 60, NOW + 300);
        assert!(matches!(
            key.public().verify_jwt(&token, AUD, NOW + 1),
            Err(JwtError::NotYetValid)
        ));
    }

    #[test]
    fn thumbprint_is_stable_for_same_key() {
        let key = PrivateKey::generate();
        let t1 = key.public().jwk_thumbprint();
        let t2 = key.public().jwk_thumbprint();
        assert_eq!(t1, t2);
    }

    #[test]
    fn thumbprint_differs_across_keys() {
        let a = PrivateKey::generate().public().jwk_thumbprint();
        let b = PrivateKey::generate().public().jwk_thumbprint();
        assert_ne!(a, b);
    }

    #[test]
    fn sign_jwt_uses_thumbprint_as_kid_and_sub_when_caller_passes_it() {
        let key = PrivateKey::generate();
        let thumb = key.public().jwk_thumbprint();
        let token = key.sign_jwt(&thumb, AUD, NOW, NOW + 300);
        // Decode header and check kid.
        let parts: Vec<&str> = token.splitn(3, '.').collect();
        let header: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header["kid"], thumb);
        // Verify also works.
        let claims = key.public().verify_jwt(&token, AUD, NOW + 1).unwrap();
        assert_eq!(claims.sub, thumb);
    }
}
