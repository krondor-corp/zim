# Vault Data Model

This page documents durable relationships and invariants. Exact Rust fields,
serde compatibility, and constructor signatures remain in source-level
Rustdoc.

## Identity

`VaultId` is the BLAKE3 hash of the genesis manifest blob. It is derived rather
than declared: manifests contain no vault ID field. Genesis includes a random
nonce so vaults created from identical content still have distinct IDs.

A claimed chain belongs to a vault only when walking its `previous` links to
genesis yields the claimed hash.

## Manifest

A manifest is a signed, DAG-CBOR-encoded, content-addressed record. It is not
encrypted, although it contains encrypted secret shares and encrypted
directory bodies.

Conceptually, it binds:

- vault uniqueness and display metadata,
- a height and link to the previous manifest,
- the current shareholder set,
- the encrypted root directory and inline directory metadata,
- pinned external blob hashes,
- an encrypted operations-log link and Lamport clock,
- the author and signature.

The signature covers the complete manifest state except the signature value.
Genesis must be signed by one of its shareholders. Later manifests must be
signed by a shareholder from the preceding manifest.

There is no separate relay list. Hosted routing is part of a share.

## Working Tree

`Vault<B, L>` combines a manifest and its link, the local private key, a
decrypted `Fs<B>` working tree, and a `VaultLog` implementation.

The filesystem is a directory tree. Each directory maps a child name to either:

- a file entry containing a ciphertext link, per-file secret, and optional
  MIME, linked-data, and plaintext-hash metadata; or
- a directory entry containing an encrypted directory-body link and
  per-directory secret.

The working tree accumulates path operations in memory. It is not independently
versioned; a save commits the filesystem and operations into a new manifest.

## Save Lifecycle

Saving a vault:

1. Generates a fresh vault secret.
2. Saves changed directory bodies and file references.
3. Re-seals the new vault secret to every shareholder.
4. Stores pending operations under the new secret.
5. Updates the root, inline metadata, pins, operation clock, previous link, and
   height.
6. Signs and stores the manifest.
7. Appends its link to the vault log.

Existing file ciphertext is not re-encrypted merely because the vault is
saved. Rewritten directory ancestors receive fresh secrets; the root uses the
new vault secret.

## Source Of Truth

The current definitions live under `crates/zim-core/src/vault/` and
`crates/zim-core/src/fs/`. When a private field or serialized shape matters,
read those definitions rather than extending this page with a copied struct.
