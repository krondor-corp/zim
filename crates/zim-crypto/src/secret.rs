//! Content encryption using ChaCha20-Poly1305
//!
//! This module provides symmetric encryption for bucket data. Each encrypted item
//! (nodes, files) has its own unique `Secret` key, providing:
//! - **Content-addressed storage**: Encrypted data can be hashed deterministically
//! - **Per-item encryption**: Compromising one key doesn't affect other items
//! - **Efficient key rotation**: Can re-encrypt specific items without touching others

use std::io::Read;
use std::ops::Deref;

use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use chacha20poly1305::Key;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use serde::{Deserialize, Serialize};

/// Size of ChaCha20-Poly1305 nonce in bytes
pub const NONCE_SIZE: usize = 12;
/// Size of ChaCha20-Poly1305 key in bytes (256 bits)
pub const SECRET_SIZE: usize = 32;
/// Size of BLAKE3 hash in bytes (256 bits)
pub const BLAKE3_HASH_SIZE: usize = 32;

/// Errors that can occur during encryption/decryption
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("secret error: {0}")]
    Default(#[from] anyhow::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// A 256-bit symmetric encryption key for content encryption
///
/// Each `Secret` is used to encrypt a single item (node or data blob) using ChaCha20-Poly1305 AEAD.
/// The encrypted format is: `nonce (12 bytes) || encrypted(hash(32 bytes) || plaintext) || tag (16 bytes)`.
/// The BLAKE3 hash of the plaintext is prepended before encryption to enable content verification
/// without full decryption (useful for filesystem sync operations).
///
/// # Examples
///
/// ```ignore
/// // Generate a new random secret
/// let secret = Secret::generate();
///
/// // Encrypt data
/// let plaintext = b"sensitive data";
/// let ciphertext = secret.encrypt(plaintext)?;
///
/// // Decrypt data
/// let recovered = secret.decrypt(&ciphertext)?;
/// assert_eq!(plaintext, &recovered[..]);
/// ```
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Secret([u8; SECRET_SIZE]);

impl Default for Secret {
    fn default() -> Self {
        Secret([0; SECRET_SIZE])
    }
}

impl Deref for Secret {
    type Target = [u8; SECRET_SIZE];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<[u8; SECRET_SIZE]> for Secret {
    fn from(bytes: [u8; SECRET_SIZE]) -> Self {
        Secret(bytes)
    }
}

impl Secret {
    /// Generate a new random secret using a cryptographically secure RNG
    pub fn generate() -> Self {
        let mut buff = [0; SECRET_SIZE];
        getrandom::getrandom(&mut buff).expect("failed to generate random bytes");
        Self(buff)
    }

    /// Create a secret from a byte slice
    ///
    /// # Errors
    ///
    /// Returns an error if the slice length is not exactly `SECRET_SIZE` bytes.
    pub fn from_slice(data: &[u8]) -> Result<Self, SecretError> {
        if data.len() != SECRET_SIZE {
            return Err(anyhow::anyhow!(
                "invalid secret size, expected {}, got {}",
                SECRET_SIZE,
                data.len()
            )
            .into());
        }
        let mut buff = [0; SECRET_SIZE];
        buff.copy_from_slice(data);
        Ok(buff.into())
    }

    /// Get a reference to the secret key bytes
    pub fn bytes(&self) -> &[u8] {
        self.0.as_ref()
    }

    /// Encrypt data using ChaCha20-Poly1305 AEAD
    ///
    /// The output format is: `nonce (12 bytes) || encrypted(hash(32) || plaintext) || auth_tag (16 bytes)`.
    /// A BLAKE3 hash of the plaintext is computed and prepended to the data before encryption.
    /// A random nonce is generated for each encryption operation.
    ///
    /// # Errors
    ///
    /// Returns an error if encryption fails (should be rare, only on system RNG failure).
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, SecretError> {
        // Compute BLAKE3 hash of plaintext
        let plaintext_hash = blake3::hash(data);

        // Prepend hash to plaintext
        let mut data_with_hash = Vec::with_capacity(BLAKE3_HASH_SIZE + data.len());
        data_with_hash.extend_from_slice(plaintext_hash.as_bytes());
        data_with_hash.extend_from_slice(data);

        let key = Key::from_slice(self.bytes());
        let cipher = ChaCha20Poly1305::new(key);

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        getrandom::getrandom(&mut nonce_bytes)
            .map_err(|e| anyhow::anyhow!("failed to generate nonce: {}", e))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data_with_hash.as_ref())
            .map_err(|_| anyhow::anyhow!("encrypt error"))?;

        let mut out = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
        out.extend_from_slice(nonce.as_ref());
        out.extend_from_slice(ciphertext.as_ref());

        Ok(out)
    }

    /// Decrypt data using ChaCha20-Poly1305 AEAD
    ///
    /// Expects input in the format: `nonce (12 bytes) || encrypted(hash(32) || plaintext) || auth_tag (16 bytes)`.
    /// Returns only the plaintext (hash is stripped but verified for integrity).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Data is too short to contain a nonce
    /// - Authentication tag verification fails (data was tampered with or wrong key)
    /// - Decrypted data is too short to contain the hash header
    /// - Hash verification fails (data corruption)
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>, SecretError> {
        if data.len() < NONCE_SIZE {
            return Err(anyhow::anyhow!("data too short for nonce").into());
        }

        let key = Key::from_slice(self.bytes());
        let nonce = Nonce::from_slice(&data[..NONCE_SIZE]);
        let cipher = ChaCha20Poly1305::new(key);
        let decrypted = cipher
            .decrypt(nonce, &data[NONCE_SIZE..])
            .map_err(|_| anyhow::anyhow!("decrypt error"))?;

        // Extract hash and plaintext
        if decrypted.len() < BLAKE3_HASH_SIZE {
            return Err(anyhow::anyhow!("decrypted data too short for hash header").into());
        }

        let stored_hash = &decrypted[..BLAKE3_HASH_SIZE];
        let plaintext = &decrypted[BLAKE3_HASH_SIZE..];

        // Verify hash integrity
        let computed_hash = blake3::hash(plaintext);
        if stored_hash != computed_hash.as_bytes() {
            return Err(anyhow::anyhow!("hash verification failed - data corrupted").into());
        }

        Ok(plaintext.to_vec())
    }

    /// Extract the BLAKE3 hash of the plaintext without decrypting the full content
    ///
    /// This is useful for filesystem sync operations where you only need to compare
    /// content hashes without loading the entire file into memory.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Data is too short to contain a nonce
    /// - Authentication tag verification fails (data was tampered with or wrong key)
    /// - Decrypted data is too short to contain the hash header
    pub fn extract_plaintext_hash(
        &self,
        data: &[u8],
    ) -> Result<[u8; BLAKE3_HASH_SIZE], SecretError> {
        if data.len() < NONCE_SIZE {
            return Err(anyhow::anyhow!("data too short for nonce").into());
        }

        let key = Key::from_slice(self.bytes());
        let nonce = Nonce::from_slice(&data[..NONCE_SIZE]);
        let cipher = ChaCha20Poly1305::new(key);
        let decrypted = cipher
            .decrypt(nonce, &data[NONCE_SIZE..])
            .map_err(|_| anyhow::anyhow!("decrypt error"))?;

        // Extract just the hash
        if decrypted.len() < BLAKE3_HASH_SIZE {
            return Err(anyhow::anyhow!("decrypted data too short for hash header").into());
        }

        let mut hash = [0u8; BLAKE3_HASH_SIZE];
        hash.copy_from_slice(&decrypted[..BLAKE3_HASH_SIZE]);

        Ok(hash)
    }

    /// Truly streaming encryption — raw ChaCha20 stream cipher,
    /// **no Poly1305 authenticator**. `read()` returns ciphertext as
    /// fast as the caller pulls it; memory stays O(1) regardless of
    /// input size.
    ///
    /// Wire format is the minimum possible:
    ///
    /// ```text
    /// nonce (12 bytes) || ciphertext (same length as plaintext)
    /// ```
    ///
    /// **Integrity here is not the cipher's job** — it's iroh-blobs'.
    /// Every blob is content-addressed by its ciphertext hash, so any
    /// tampering en route changes the hash and the fetch fails before
    /// the decrypt ever runs. The plaintext hash recorded on
    /// `Entry::File` is the final cross-check after decrypt for
    /// pipeline bugs / silent storage corruption.
    ///
    /// **Not compatible** with [`Self::encrypt`]'s one-shot output —
    /// that path carries an AEAD tag + embedded `blake3(plaintext)`
    /// integrity envelope, and stays in place for small structured
    /// data (dir bodies, manifest fields) where buffering isn't a
    /// concern.
    pub fn encrypt_reader<R>(&self, reader: R) -> Result<impl Read, SecretError>
    where
        R: Read,
    {
        StreamingCipherReader::new_encrypt(self, reader)
    }

    /// Pair of [`Self::encrypt_reader`]. Reads the leading 12-byte
    /// nonce off the ciphertext stream and yields plaintext via the
    /// raw ChaCha20 keystream. Same wire format, same memory bound.
    pub fn decrypt_reader<R>(&self, reader: R) -> Result<impl Read, SecretError>
    where
        R: Read,
    {
        StreamingCipherReader::new_decrypt(self, reader)
    }
}

// ─── Raw streaming cipher adapter ─────────────────────────────────

/// Wraps a plaintext or ciphertext `Read` and applies the ChaCha20
/// keystream byte-by-byte. ChaCha20 is symmetric (XOR), so the
/// encrypt and decrypt cases share the same body — they differ only
/// in where the nonce comes from (fresh-random for encrypt, parsed
/// from the head of the stream for decrypt) and whether the nonce is
/// prepended to the output stream.
struct StreamingCipherReader<R: Read> {
    inner: R,
    cipher: ChaCha20,
    /// On encrypt, the 12-byte nonce we generated — drained into the
    /// caller's buffer before any ciphertext bytes flow. `None` once
    /// drained, or always-`None` on the decrypt side (nonce already
    /// consumed from the inner reader by `new_decrypt`).
    nonce_prefix: Option<NoncePrefix>,
}

/// Tiny owned byte stack to drain the nonce header into the caller
/// on encrypt without dragging in a Vec / VecDeque.
struct NoncePrefix {
    bytes: [u8; NONCE_SIZE],
    /// How many bytes have already been handed to the caller.
    consumed: usize,
}

impl<R: Read> StreamingCipherReader<R> {
    fn new_encrypt(secret: &Secret, inner: R) -> Result<Self, SecretError> {
        let mut nonce = [0u8; NONCE_SIZE];
        getrandom::getrandom(&mut nonce)
            .map_err(|e| anyhow::anyhow!("failed to generate nonce: {e}"))?;
        let cipher = ChaCha20::new(secret.bytes().into(), (&nonce).into());
        Ok(Self {
            inner,
            cipher,
            nonce_prefix: Some(NoncePrefix {
                bytes: nonce,
                consumed: 0,
            }),
        })
    }

    fn new_decrypt(secret: &Secret, mut inner: R) -> Result<Self, SecretError> {
        let mut nonce = [0u8; NONCE_SIZE];
        inner.read_exact(&mut nonce).map_err(SecretError::Io)?;
        let cipher = ChaCha20::new(secret.bytes().into(), (&nonce).into());
        Ok(Self {
            inner,
            cipher,
            nonce_prefix: None,
        })
    }
}

impl<R: Read> Read for StreamingCipherReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Encrypt-side: first drain the nonce prefix into the caller
        // before any cipher bytes start flowing. The caller will see
        // exactly the header we generated, untouched by the cipher.
        if let Some(prefix) = &mut self.nonce_prefix {
            let remaining = &prefix.bytes[prefix.consumed..];
            if !remaining.is_empty() {
                let n = buf.len().min(remaining.len());
                buf[..n].copy_from_slice(&remaining[..n]);
                prefix.consumed += n;
                if prefix.consumed == NONCE_SIZE {
                    self.nonce_prefix = None;
                }
                return Ok(n);
            }
        }

        // Read raw bytes from the inner stream, XOR with the
        // keystream, return. ChaCha20's `apply_keystream` operates
        // in place — same code path for encrypt and decrypt.
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.cipher.apply_keystream(&mut buf[..n]);
        }
        Ok(n)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_secret_encrypt_decrypt() {
        let secret = Secret::generate();
        let data = b"hello world, this is a test message for encryption";

        let encrypted = secret.encrypt(data).unwrap();
        let decrypted = secret.decrypt(&encrypted).unwrap();

        assert_eq!(data.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_decrypt_reader() {
        let secret = Secret::generate();
        let data = b"hello world, this is a test message for reader encryption and decryption";

        // Create encrypted reader
        let reader = Cursor::new(data.to_vec());
        let mut encrypted_reader = secret.encrypt_reader(reader).unwrap();

        // Read encrypted data
        let mut encrypted_data = Vec::new();
        encrypted_reader.read_to_end(&mut encrypted_data).unwrap();

        // Decrypt using reader
        let encrypted_cursor = Cursor::new(encrypted_data);
        let mut decrypted_reader = secret.decrypt_reader(encrypted_cursor).unwrap();

        let mut decrypted_data = Vec::new();
        decrypted_reader.read_to_end(&mut decrypted_data).unwrap();

        assert_eq!(data.to_vec(), decrypted_data);
    }

    /// Helper: round-trip `data` through encrypt_reader → decrypt_reader
    /// and assert the result equals the input.
    fn roundtrip(secret: &Secret, data: &[u8]) {
        let mut ciphertext = Vec::new();
        secret
            .encrypt_reader(Cursor::new(data.to_vec()))
            .unwrap()
            .read_to_end(&mut ciphertext)
            .unwrap();
        // Wire shape: 12-byte nonce header + raw-cipher body (same length as plaintext).
        assert_eq!(
            ciphertext.len(),
            NONCE_SIZE + data.len(),
            "ciphertext should be nonce + plaintext length, not chunked or tagged"
        );

        let mut roundtripped = Vec::new();
        secret
            .decrypt_reader(Cursor::new(ciphertext))
            .unwrap()
            .read_to_end(&mut roundtripped)
            .unwrap();
        assert_eq!(roundtripped, data, "round-trip preserves bytes exactly");
    }

    #[test]
    fn streaming_round_trips_empty_input() {
        roundtrip(&Secret::generate(), b"");
    }

    #[test]
    fn streaming_round_trips_single_byte() {
        roundtrip(&Secret::generate(), b"x");
    }

    #[test]
    fn streaming_round_trips_at_typical_chunk_boundaries() {
        let secret = Secret::generate();
        // The internal buffer doesn't slice plaintext, but make sure
        // typical I/O-aligned sizes still round-trip.
        for size in [4096usize, 8192, 65536, 65537] {
            let body: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            roundtrip(&secret, &body);
        }
    }

    #[test]
    fn streaming_round_trips_large_input() {
        // 1 MiB — well past anything you'd want to buffer in RAM.
        let body: Vec<u8> = (0..1024 * 1024).map(|i| (i % 251) as u8).collect();
        roundtrip(&Secret::generate(), &body);
    }

    #[test]
    fn streaming_uses_fresh_nonces() {
        // Two encrypts of the same plaintext under the same key must
        // produce different ciphertexts — otherwise we'd be reusing a
        // nonce, which is fatal for any stream cipher.
        let secret = Secret::generate();
        let data = b"same body, different nonce please";

        let mut a = Vec::new();
        secret
            .encrypt_reader(Cursor::new(data.to_vec()))
            .unwrap()
            .read_to_end(&mut a)
            .unwrap();
        let mut b = Vec::new();
        secret
            .encrypt_reader(Cursor::new(data.to_vec()))
            .unwrap()
            .read_to_end(&mut b)
            .unwrap();

        assert_ne!(a[..NONCE_SIZE], b[..NONCE_SIZE], "nonces must differ");
        assert_ne!(
            a[NONCE_SIZE..],
            b[NONCE_SIZE..],
            "with fresh nonces the ciphertext bodies must differ"
        );
    }

    #[test]
    fn test_secret_size_validation() {
        let too_short = [1u8; 16];
        let too_long = [1u8; 64];

        assert!(Secret::from_slice(&too_short).is_err());
        assert!(Secret::from_slice(&too_long).is_err());

        let just_right = [1u8; SECRET_SIZE];
        assert!(Secret::from_slice(&just_right).is_ok());
    }

    #[test]
    fn test_extract_plaintext_hash() {
        let secret = Secret::generate();
        let data = b"test data for hash extraction";

        // Encrypt the data
        let encrypted = secret.encrypt(data).unwrap();

        // Extract the hash without full decryption
        let extracted_hash = secret.extract_plaintext_hash(&encrypted).unwrap();

        // Compute expected hash
        let expected_hash = blake3::hash(data);

        assert_eq!(extracted_hash, *expected_hash.as_bytes());
    }

    #[test]
    fn test_hash_verification_on_decrypt() {
        let secret = Secret::generate();
        let data = b"test data for integrity check";

        // Encrypt the data
        let mut encrypted = secret.encrypt(data).unwrap();

        // Decrypt should succeed with valid data
        let decrypted = secret.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, data.to_vec());

        // Corrupt the encrypted data (modify a byte in the ciphertext region)
        // Note: This should fail authentication, not hash verification
        if encrypted.len() > NONCE_SIZE + 16 {
            encrypted[NONCE_SIZE + 10] ^= 0xFF;

            // This should fail during ChaCha20-Poly1305 authentication
            let result = secret.decrypt(&encrypted);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_empty_data_encryption() {
        let secret = Secret::generate();
        let data = b"";

        let encrypted = secret.encrypt(data).unwrap();
        let decrypted = secret.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, data.to_vec());

        // Hash should still be extractable
        let hash = secret.extract_plaintext_hash(&encrypted).unwrap();
        let expected_hash = blake3::hash(data);
        assert_eq!(hash, *expected_hash.as_bytes());
    }
}
