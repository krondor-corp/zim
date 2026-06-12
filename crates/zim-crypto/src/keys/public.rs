use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};

use super::{KeyError, PUBLIC_KEY_SIZE};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy)]
pub struct PublicKey(VerifyingKey);

impl PartialOrd for PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PublicKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

impl From<[u8; PUBLIC_KEY_SIZE]> for PublicKey {
    fn from(bytes: [u8; PUBLIC_KEY_SIZE]) -> Self {
        PublicKey(VerifyingKey::from_bytes(&bytes).expect("valid public key"))
    }
}

impl From<VerifyingKey> for PublicKey {
    fn from(key: VerifyingKey) -> Self {
        PublicKey(key)
    }
}

impl TryFrom<&[u8]> for PublicKey {
    type Error = KeyError;
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != PUBLIC_KEY_SIZE {
            return Err(anyhow::anyhow!(
                "invalid public key size, expected {}, got {}",
                PUBLIC_KEY_SIZE,
                bytes.len()
            )
            .into());
        }
        let mut buff = [0; PUBLIC_KEY_SIZE];
        buff.copy_from_slice(bytes);
        Ok(buff.into())
    }
}

impl PublicKey {
    pub fn from_hex(hex: &str) -> Result<Self, KeyError> {
        let hex = hex.strip_prefix("0x").unwrap_or(hex);
        let mut buff = [0; PUBLIC_KEY_SIZE];
        hex::decode_to_slice(hex, &mut buff)
            .map_err(|_| anyhow::anyhow!("public key hex decode error"))?;
        // Use `TryFrom<&[u8]>` rather than `From<[u8; 32]>` — the
        // latter panics on non-curve-point bytes, which makes this
        // method unsafe to call on user input (form payloads, URL
        // params, etc.). The TryFrom path turns "not a valid Edwards
        // point" into a typed error the caller can render.
        Self::try_from(&buff[..])
    }

    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_SIZE] {
        *self.0.as_bytes()
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    pub fn verify(
        &self,
        msg: &[u8],
        signature: &ed25519_dalek::Signature,
    ) -> Result<(), ed25519_dalek::SignatureError> {
        self.0.verify_strict(msg, signature)
    }

    /// Convenience wrapper around [`Self::verify`] that takes a raw
    /// 64-byte signature — useful for callers parsing signatures off
    /// the wire (HTTP bodies, JSON payloads) without depending on
    /// the `ed25519_dalek` Signature type.
    pub fn verify_bytes(
        &self,
        msg: &[u8],
        signature: &[u8; 64],
    ) -> Result<(), ed25519_dalek::SignatureError> {
        let sig = ed25519_dalek::Signature::from_bytes(signature);
        self.0.verify_strict(msg, &sig)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_hex_roundtrip() {
        let bytes = [42u8; PUBLIC_KEY_SIZE];
        let key = PublicKey::from_hex(&hex::encode(bytes)).unwrap();
        assert_eq!(key.to_bytes(), bytes);
        assert_eq!(key.to_hex(), hex::encode(bytes));
    }

    #[test]
    fn test_from_hex_with_prefix() {
        let bytes = [42u8; PUBLIC_KEY_SIZE];
        let hex_str = format!("0x{}", hex::encode(bytes));
        let key = PublicKey::from_hex(&hex_str).unwrap();
        assert_eq!(key.to_bytes(), bytes);
    }

    #[test]
    fn test_try_from_slice_invalid_size() {
        let too_short = [1u8; 16];
        assert!(PublicKey::try_from(too_short.as_slice()).is_err());

        let too_long = [1u8; 64];
        assert!(PublicKey::try_from(too_long.as_slice()).is_err());
    }
}
