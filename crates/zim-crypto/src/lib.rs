//! Cryptographic primitives for Zim.
//!
//! - **Identity**: Ed25519 keypairs (`SecretKey`/`PublicKey`).
//! - **Content encryption**: ChaCha20-Poly1305 with per-item `Secret` keys.
//! - **Key sharing**: X25519 ECDH + AES-KW for sharing per-item `Secret`s with peers.

mod keys;
mod secret;
mod secret_share;

pub use ed25519_dalek::Signature;
pub use keys::{PublicKey, SecretKey};
pub use secret::{Secret, SecretError, BLAKE3_HASH_SIZE};
pub use secret_share::{SecretShare, SecretShareError};
