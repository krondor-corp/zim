#[cfg(feature = "iroh-keys")]
use std::ops::Deref;

use curve25519_dalek::edwards::CompressedEdwardsY;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

// Inner key types swap between iroh's wrappers (default, used by every native
// binary) and ed25519-dalek directly (wasm builds, where iroh's networking
// stack does not compile to wasm32-unknown-unknown).
#[cfg(not(feature = "iroh-keys"))]
use ed25519_dalek::{SigningKey as SSecretKey, VerifyingKey as PPublicKey};
#[cfg(feature = "iroh-keys")]
use iroh::{PublicKey as PPublicKey, SecretKey as SSecretKey};

/// Size of Ed25519 private key in bytes
pub const PRIVATE_KEY_SIZE: usize = 32;
/// Size of Ed25519 public key in bytes
pub const PUBLIC_KEY_SIZE: usize = 32;

/// Errors that can occur during key operations
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("key error: {0}")]
    Default(#[from] anyhow::Error),
}

/// Public key for peer identity, key sharing, and update provenance.
///
/// Wraps Ed25519. With the default `iroh-keys` feature this is an iroh
/// `PublicKey` (also iroh's `NodeId`); under the `wasm` feature it is an
/// `ed25519_dalek::VerifyingKey`. The public API on `PublicKey` is the same
/// either way.
#[cfg_attr(
    feature = "iroh-keys",
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Serialize,
        Deserialize,
        PartialOrd,
        Ord,
        Copy
    )
)]
#[cfg_attr(
    not(feature = "iroh-keys"),
    derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy)
)]
pub struct PublicKey(PPublicKey);

#[cfg(not(feature = "iroh-keys"))]
impl PartialOrd for PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(not(feature = "iroh-keys"))]
impl Ord for PublicKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

#[cfg(feature = "iroh-keys")]
impl Deref for PublicKey {
    type Target = PPublicKey;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "iroh-keys")]
impl From<PPublicKey> for PublicKey {
    fn from(key: PPublicKey) -> Self {
        PublicKey(key)
    }
}

#[cfg(feature = "iroh-keys")]
impl From<PublicKey> for PPublicKey {
    fn from(key: PublicKey) -> Self {
        key.0
    }
}

impl From<[u8; PUBLIC_KEY_SIZE]> for PublicKey {
    fn from(bytes: [u8; PUBLIC_KEY_SIZE]) -> Self {
        PublicKey(PPublicKey::from_bytes(&bytes).expect("valid public key"))
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
    /// Parse a public key from a hexadecimal string
    ///
    /// Accepts both plain hex and "0x"-prefixed hex strings.
    pub fn from_hex(hex: &str) -> Result<Self, KeyError> {
        let hex = hex.strip_prefix("0x").unwrap_or(hex);
        let mut buff = [0; PUBLIC_KEY_SIZE];
        hex::decode_to_slice(hex, &mut buff)
            .map_err(|_| anyhow::anyhow!("public key hex decode error"))?;
        Ok(buff.into())
    }

    /// Convert public key to raw bytes
    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_SIZE] {
        *self.0.as_bytes()
    }

    /// Convert public key to hexadecimal string
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// Convert Ed25519 public key to X25519 (Montgomery curve) for ECDH.
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_x25519(&self) -> Result<X25519PublicKey, KeyError> {
        let edwards_bytes = self.to_bytes();
        let edwards_point = CompressedEdwardsY::from_slice(&edwards_bytes)
            .map_err(|_| anyhow::anyhow!("public key invalid edwards point"))?
            .decompress()
            .ok_or_else(|| anyhow::anyhow!("public key failed to decompress edwards point"))?;

        let montgomery_point = edwards_point.to_montgomery();
        Ok(X25519PublicKey::from(montgomery_point.to_bytes()))
    }

    /// Verify an Ed25519 signature on a message.
    pub fn verify(
        &self,
        msg: &[u8],
        signature: &ed25519_dalek::Signature,
    ) -> Result<(), ed25519_dalek::SignatureError> {
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&self.to_bytes())?;
        verifying_key.verify_strict(msg, signature)
    }
}

/// Secret key for peer identity and key sharing.
///
/// Wraps Ed25519. With the default `iroh-keys` feature this is an iroh
/// `SecretKey`; under the `wasm` feature it is an `ed25519_dalek::SigningKey`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretKey(pub SSecretKey);

impl From<[u8; PRIVATE_KEY_SIZE]> for SecretKey {
    fn from(secret: [u8; PRIVATE_KEY_SIZE]) -> Self {
        Self(SSecretKey::from_bytes(&secret))
    }
}

#[cfg(feature = "iroh-keys")]
impl Deref for SecretKey {
    type Target = SSecretKey;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl SecretKey {
    /// Parse a secret key from a hexadecimal string
    pub fn from_hex(hex: &str) -> Result<Self, KeyError> {
        let hex = hex.strip_prefix("0x").unwrap_or(hex);
        let mut buff = [0; PRIVATE_KEY_SIZE];
        hex::decode_to_slice(hex, &mut buff)
            .map_err(|_| anyhow::anyhow!("private key hex decode error"))?;
        Ok(Self::from(buff))
    }

    /// Generate a new random secret key using a cryptographically secure RNG
    pub fn generate() -> Self {
        let mut bytes = [0u8; PRIVATE_KEY_SIZE];
        getrandom::getrandom(&mut bytes).expect("failed to generate random bytes");
        Self::from(bytes)
    }

    /// Derive the public key from this secret key
    pub fn public(&self) -> PublicKey {
        #[cfg(feature = "iroh-keys")]
        return PublicKey(self.0.public());
        #[cfg(not(feature = "iroh-keys"))]
        return PublicKey(self.0.verifying_key());
    }

    /// Convert secret key to raw bytes
    pub fn to_bytes(&self) -> [u8; PRIVATE_KEY_SIZE] {
        self.0.to_bytes()
    }

    /// Convert secret key to hexadecimal string
    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    /// Encode secret key in PEM format for secure storage
    pub fn to_pem(&self) -> String {
        let pem = pem::Pem::new("PRIVATE KEY", self.to_bytes());
        pem::encode(&pem)
    }

    /// Parse a secret key from PEM format
    pub fn from_pem(pem_str: &str) -> Result<Self, KeyError> {
        let pem = pem::parse(pem_str).map_err(|e| anyhow::anyhow!("failed to parse PEM: {}", e))?;

        if pem.tag() != "PRIVATE KEY" {
            return Err(anyhow::anyhow!("invalid PEM tag, expected PRIVATE KEY").into());
        }

        let contents = pem.contents();
        if contents.len() != PRIVATE_KEY_SIZE {
            return Err(anyhow::anyhow!(
                "invalid private key size in PEM, expected {}, got {}",
                PRIVATE_KEY_SIZE,
                contents.len()
            )
            .into());
        }

        let mut bytes = [0u8; PRIVATE_KEY_SIZE];
        bytes.copy_from_slice(contents);
        Ok(Self::from(bytes))
    }

    /// Convert Ed25519 secret key to X25519 (Montgomery curve) for ECDH.
    pub(crate) fn to_x25519(&self) -> StaticSecret {
        #[cfg(feature = "iroh-keys")]
        {
            let signing_key = self.0.secret();
            let scalar_bytes = signing_key.to_scalar_bytes();
            StaticSecret::from(scalar_bytes)
        }
        #[cfg(not(feature = "iroh-keys"))]
        {
            let scalar_bytes = self.0.to_scalar_bytes();
            StaticSecret::from(scalar_bytes)
        }
    }

    /// Sign a message with this secret key using Ed25519.
    pub fn sign(&self, msg: &[u8]) -> ed25519_dalek::Signature {
        #[cfg(feature = "iroh-keys")]
        {
            // iroh ships its own (older) ed25519_dalek; convert via raw bytes
            // since both versions share the 64-byte representation.
            let sig = self.0.sign(msg);
            ed25519_dalek::Signature::from_bytes(&sig.to_bytes())
        }
        #[cfg(not(feature = "iroh-keys"))]
        {
            use ed25519_dalek::Signer;
            self.0.sign(msg)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let private_key = SecretKey::generate();
        let public_key = private_key.public();

        // Test round-trip conversion
        let private_hex = private_key.to_hex();
        let recovered_private = SecretKey::from_hex(&private_hex).unwrap();
        assert_eq!(private_key.to_bytes(), recovered_private.to_bytes());

        let public_hex = public_key.to_hex();
        let recovered_public = PublicKey::from_hex(&public_hex).unwrap();
        assert_eq!(public_key.to_bytes(), recovered_public.to_bytes());
    }

    #[test]
    fn test_pem_serialization() {
        let private_key = SecretKey::generate();

        // Test round-trip PEM conversion
        let pem = private_key.to_pem();
        let recovered_private = SecretKey::from_pem(&pem).unwrap();
        assert_eq!(private_key.to_bytes(), recovered_private.to_bytes());

        // Verify the recovered key can produce the same public key
        assert_eq!(
            private_key.public().to_bytes(),
            recovered_private.public().to_bytes()
        );
    }

    #[test]
    fn test_sign_and_verify() {
        let secret_key = SecretKey::generate();
        let public_key = secret_key.public();
        let message = b"hello, world!";

        // Sign the message
        let signature = secret_key.sign(message);

        // Verify the signature
        assert!(public_key.verify(message, &signature).is_ok());

        // Verify fails with wrong message
        let wrong_message = b"hello, world?";
        assert!(public_key.verify(wrong_message, &signature).is_err());

        // Verify fails with wrong key
        let other_key = SecretKey::generate().public();
        assert!(other_key.verify(message, &signature).is_err());
    }
}
