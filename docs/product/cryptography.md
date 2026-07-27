# Cryptography

Zim uses cryptography to separate storage and transport from authority to read
or modify a vault. This page describes the resulting guarantees, not the Rust
types or serialized formats.

## Device Identity

Every native peer and browser identity has an Ed25519 key. Signatures identify
manifest authors, authenticate peer traffic, and let daemons mint short-lived
hub credentials without storing a reusable bearer token.

Private identity keys remain on the device that owns them. Native daemon and
hub keys are stored locally without passphrase encryption, so host filesystem
security remains part of the trust boundary.

## Vault Access

A vault secret is sealed separately to each shareholder using X25519 key
agreement and AES key wrapping. Possessing one sealed grant is insufficient;
the recipient's private key is required to recover the vault secret.

Every save generates a new vault secret and reissues grants to the current
shareholders. Removing a shareholder excludes it from future grants, but does
not erase secrets, plaintext, or historical versions it already obtained.

## Content Encryption

Directory bodies and operation logs use authenticated ChaCha20-Poly1305
encryption. File bodies use streaming ChaCha20 with a fresh nonce so large
files can be processed incrementally.

File-body encryption is not AEAD. Integrity relies on the content-addressed
hash of the ciphertext. A stored plaintext hash supports comparison but is not
the normal read-time integrity boundary.

Manifests are signed rather than encrypted. This allows peers and hubs to
verify authorship and traverse history without receiving vault secrets.

## Content Addressing

Encrypted blobs are addressed by BLAKE3 hashes. A peer can verify that received
ciphertext matches the requested address regardless of which peer or hub
served it.

Fresh nonces mean re-encrypting identical plaintext generally produces a new
ciphertext hash. Content addressing verifies exact stored bytes; it does not
make plaintext identities globally stable.

## Limitations

- Zim has not received an independent cryptographic or protocol audit.
- Compromise of an authorized device exposes content available to that device.
- Revocation cannot retract historical access.
- Weak browser passphrases remain vulnerable to offline guessing after escrow
  ciphertext is stolen.
- File-body integrity depends on ciphertext addressing rather than an
  authentication tag.
