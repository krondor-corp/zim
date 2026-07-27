//! `did:key` codec for ed25519 public keys.
//!
//! Wire format per the [did:key spec][spec]:
//!
//! ```text
//! did:key:<multibase-base58btc>(<multicodec-prefix><raw-pubkey-bytes>)
//! ```
//!
//! - Multibase layer: the `multibase` crate (already in-tree via
//!   `serde_ipld_dagcbor → ipld-core → cid`), `z` = base58btc.
//! - Multicodec prefix for ed25519 is `0xed 0x01` (varint-encoded
//!   `0xed`). Hard-coded — the surface is two bytes and never changes.
//!
//! [spec]: https://w3c-ccg.github.io/did-method-key/

use multibase::Base;
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
    multibase::encode(Base::Base58Btc, prefixed)
}

/// Decode a `did:key` identifier (the substring after `did:key:`) back
/// into an [`PublicKey`]. Errors describe what went wrong without
/// leaking the partial bytes — these strings flow into user-visible
/// validation messages.
pub fn did_key_decode(identifier: &str) -> Result<PublicKey, String> {
    let (base, bytes) = multibase::decode(identifier).map_err(|e| format!("multibase: {e}"))?;
    if base != Base::Base58Btc {
        return Err("expected `z` (base58btc) multibase prefix".to_string());
    }
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
    fn decodes_the_w3c_test_vector() {
        // From the did:key spec suite — a known-good ed25519 identifier.
        did_key_decode("z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK").expect("spec vector");
    }

    #[test]
    fn rejects_non_base58btc_multibase() {
        // `f` = base16 multibase; valid multibase, wrong base for did:key.
        let err = did_key_decode("fed01aabb").unwrap_err();
        assert!(err.contains("base58btc"));
    }

    #[test]
    fn rejects_invalid_base58_chars() {
        // `0`, `O`, `I`, `l` are outside the base58btc alphabet.
        assert!(did_key_decode("z0OIl").is_err());
    }

    #[test]
    fn rejects_wrong_codec_prefix() {
        // Encode with a bogus 2-byte prefix instead of ed25519's.
        let mut bytes = vec![0x12, 0x34];
        bytes.extend([0u8; 32]);
        let encoded = multibase::encode(Base::Base58Btc, bytes);
        let err = did_key_decode(&encoded).unwrap_err();
        assert!(err.contains("unsupported multicodec prefix"));
    }
}
