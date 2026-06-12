//! `did:key` codec for ed25519 public keys.
//!
//! Wire format per the [did:key spec][spec]:
//!
//! ```text
//! did:key:<multibase-base58btc>(<multicodec-prefix><raw-pubkey-bytes>)
//! ```
//!
//! - Multicodec prefix for ed25519 is `0xed 0x01` (varint-encoded
//!   `0xed`). It's hard-coded here rather than pulled from a registry
//!   crate — the surface is two bytes and never changes.
//! - Multibase prefix is `z` (base58btc).
//!
//! This module is the only place in the workspace that touches
//! base58btc encoding. We use a minimal in-line implementation to
//! avoid a heavy multibase/multicodec dep just for two bytes of
//! prefix.
//!
//! [spec]: https://w3c-ccg.github.io/did-method-key/

use zim_crypto::PublicKey;

/// Multicodec varint prefix for ed25519-pub.
const ED25519_MULTICODEC_PREFIX: [u8; 2] = [0xed, 0x01];

/// Encode an ed25519 [`PublicKey`] as the identifier portion of a
/// `did:key` URL — the `z…` substring. The full DID is
/// `did:key:<this>`.
pub fn did_key_encode(pk: &PublicKey) -> String {
    let raw = pk.to_bytes();
    let mut prefixed = Vec::with_capacity(2 + raw.len());
    prefixed.extend_from_slice(&ED25519_MULTICODEC_PREFIX);
    prefixed.extend_from_slice(&raw);
    let mut out = String::with_capacity(1 + prefixed.len() * 2);
    out.push('z');
    out.push_str(&base58btc_encode(&prefixed));
    out
}

/// Decode a `did:key` identifier (the substring after `did:key:`) back
/// into an [`PublicKey`]. Errors describe what went wrong without
/// leaking the partial bytes — these strings flow into user-visible
/// validation messages.
pub fn did_key_decode(identifier: &str) -> Result<PublicKey, String> {
    let body = identifier
        .strip_prefix('z')
        .ok_or_else(|| "expected `z` (base58btc) multibase prefix".to_string())?;
    let bytes = base58btc_decode(body).map_err(|e| format!("base58btc decode: {e}"))?;
    let (codec, raw) = bytes
        .split_first_chunk::<2>()
        .ok_or_else(|| "too short for multicodec prefix + key".to_string())?;
    if codec != &ED25519_MULTICODEC_PREFIX {
        return Err(format!(
            "unsupported multicodec prefix {:02x?}; only ed25519 ({:02x?}) is supported",
            codec, ED25519_MULTICODEC_PREFIX
        ));
    }
    let raw: [u8; 32] = raw
        .try_into()
        .map_err(|_| format!("expected 32-byte ed25519 key, got {}", raw.len()))?;
    PublicKey::from_hex(&hex::encode(raw)).map_err(|e| format!("ed25519 from bytes: {e}"))
}

/// Used by [`crate::Did::parse`] to fail fast on garbled identifiers.
pub(crate) fn validate_did_key_identifier(identifier: &str) -> Result<(), String> {
    did_key_decode(identifier).map(|_| ())
}

// ─── Minimal base58btc (Bitcoin alphabet) ─────────────────────────
//
// We don't pull in the `bs58` crate just for this. The base58btc
// alphabet is identical to Bitcoin's, and the algorithm is small
// enough to inline. The implementation prioritises clarity over
// throughput; `did:key` identifiers are <= 50 bytes, so a hot path
// here is a non-issue.

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

fn base58btc_encode(bytes: &[u8]) -> String {
    let mut zeros = 0;
    for &b in bytes {
        if b == 0 {
            zeros += 1;
        } else {
            break;
        }
    }

    let mut digits: Vec<u8> = Vec::with_capacity(bytes.len() * 2);
    for &b in &bytes[zeros..] {
        let mut carry = b as u32;
        for d in digits.iter_mut() {
            carry += (*d as u32) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }

    let mut out = String::with_capacity(zeros + digits.len());
    for _ in 0..zeros {
        out.push(BASE58_ALPHABET[0] as char);
    }
    for d in digits.iter().rev() {
        out.push(BASE58_ALPHABET[*d as usize] as char);
    }
    out
}

fn base58btc_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut zeros = 0;
    for c in s.chars() {
        if c == BASE58_ALPHABET[0] as char {
            zeros += 1;
        } else {
            break;
        }
    }

    let mut bytes: Vec<u8> = Vec::with_capacity(s.len());
    for c in s[zeros..].chars() {
        let mut carry = match BASE58_ALPHABET.iter().position(|&a| a as char == c) {
            Some(i) => i as u32,
            None => return Err(format!("invalid base58 character `{c}`")),
        };
        for b in bytes.iter_mut() {
            carry += (*b as u32) * 58;
            *b = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    bytes.reverse();

    let mut out = Vec::with_capacity(zeros + bytes.len());
    out.resize(zeros, 0);
    out.extend(bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zim_crypto::PrivateKey;

    #[test]
    fn encode_decode_roundtrip() {
        for _ in 0..16 {
            let sk = PrivateKey::generate();
            let pk = sk.public();
            let encoded = did_key_encode(&pk);
            assert!(encoded.starts_with('z'), "must start with z (base58btc)");
            let decoded = did_key_decode(&encoded).expect("decode");
            assert_eq!(decoded.to_bytes(), pk.to_bytes());
        }
    }

    #[test]
    fn rejects_missing_z_prefix() {
        let err = did_key_decode("abc").unwrap_err();
        assert!(err.contains("multibase prefix"));
    }

    #[test]
    fn rejects_invalid_base58_char() {
        let err = did_key_decode("z0OIl").unwrap_err();
        assert!(err.contains("invalid base58 character"));
    }

    #[test]
    fn rejects_wrong_codec_prefix() {
        // Encode with a bogus 2-byte prefix instead of ed25519's.
        let mut bytes = vec![0x12, 0x34];
        bytes.extend([0u8; 32]);
        let encoded = format!("z{}", base58btc_encode(&bytes));
        let err = did_key_decode(&encoded).unwrap_err();
        assert!(err.contains("unsupported multicodec prefix"));
    }
}
