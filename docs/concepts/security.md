# Security

Zim's security model, threat model, and trust assumptions — covering both the P2P bucket layer and the zim-hub identity vault.

## Threat Model

### Zim protects against

**Untrusted storage (blobs at rest)**
- All blobs are encrypted with per-file ChaCha20-Poly1305 keys before leaving the device.
- Storage backends (local disk, S3/MinIO, relay peers) see only ciphertext + BLAKE3 hashes.

**Passive network observers**
- iroh's QUIC transport provides TLS 1.3 encryption.
- Peer connections are mutually authenticated by Ed25519 identity.

**Unauthorized peers**
- Only peers listed in `manifest.shares` hold a `SecretShare` envelope and can decrypt.
- Relay peers (listed in `manifest.relays`) pin ciphertext only — they never hold the bucket secret.
- ECDH + AES-KW ensures only the intended recipient can unwrap a share.

**Tampered data**
- AEAD (ChaCha20-Poly1305) detects modifications.
- Content addressing (BLAKE3) ensures integrity of every blob.

**Compromised hub server (identity vault layer)**
- The hub stores viewer private keys as Argon2id-encrypted blobs. Plaintext keys never exist server-side.
- A fully compromised hub (root + DB + TLS) leaks ciphertext + Google identities, but needs each viewer's password to decrypt.
- The hub operator cannot impersonate viewers in the protocol — viewer keys never leave the browser as plaintext.

### Zim does NOT protect against

**Compromised peer with valid access**
- If an authorized peer is compromised, the attacker gains bucket access.
- Per-file secret rotation (`rotate_file` / `rotate_folder`) provides forward revocation for published content; full re-keying of `SecretShare` envelopes is not yet implemented.

**Swapped JS/WASM bundle (the load-bearing trust assumption)**
- The hub serves the zim-wasm bundle that handles client-side key unlock and decryption.
- A compromised hub can replace this bundle with a phishing variant that exfiltrates the viewer's password.
- **Mitigations**: SRI hashes on `<script>` tags for `zim_wasm.js` and `datastar.min.js` (computed at build time, baked into templates); strict CSP `script-src 'self'`; HSTS forcing HTTPS. These are implementation requirements, not optional.
- Viewers can additionally verify the bundle hash out-of-band against the vendored source in the repo.

**Compromised Google account**
- An attacker with the viewer's Google session can log into zim-hub and see the encrypted blob — but still needs the unlock password (second factor).
- If the viewer uses the same password for Google and for the hub unlock: single-credential compromise.

**Metadata leakage**
- Bucket structure (file count, sizes, directory hierarchy) is visible in the encrypted tree shape.
- Relay peers can observe blob access patterns.

**Traffic analysis**
- Connection patterns may reveal peer relationships.
- Sync frequency leaks activity patterns.

## Trust layers

| Layer | What it protects | Trust assumption |
|---|---|---|
| **Bucket encryption** (ChaCha20-Poly1305 per file/dir) | Content at rest and in transit | Holder of the `Secret` can decrypt. |
| **Share envelopes** (ECDH + AES-KW) | Bucket secret distribution | Only the share recipient's private key can unwrap. |
| **Hub identity vault** (Argon2id + ChaCha20) | Viewer private keys at rest on the hub | Only the viewer's password can derive the KEK. Hub never sees plaintext. |
| **SRI + CSP** | Integrity of the browser-side decryption code | The served JS/WASM matches the build-time hash. |
| **TLS 1.3 (iroh QUIC)** | Data in transit between peers | Standard TLS trust model. |

## Per-file/folder publication

Published entries expose specific files or directories without exposing the bucket-wide secret. Each published path carries its own `Leaf` (link + secret). Rotation generates a fresh secret — actual read revocation.

Anonymous readers of published content never hold a `SecretShare` and cannot access unpublished paths. See [Access Model](./access-model.md) for the full publication design.

## Device model

Each physical device (browser, CLI, mobile) has its own Ed25519 key pair:

- **Web device**: key encrypted in the hub's identity vault (Argon2id-derived KEK).
- **Native device**: key generated on-device, private key never leaves. Hub stores only the public key.

The `Share::dialable` flag distinguishes network-reachable peers (`true`) from browser-resident web-keys (`false`). See [Identity](./identity.md) for the full device model.

## Best practices

1. **Protect secret keys** — `chmod 600` on `secret.pem` / `device.key`. Back up securely.
2. **Use a unique hub unlock password** — not your Google password. ≥16 characters recommended (Argon2id with 19 MB memory cost makes brute force expensive, but weak passwords are still weak).
3. **Audit shares and devices** — review `manifest.shares` and `/account/devices` regularly. Revoke access promptly when devices are lost or compromised.
4. **Rotate published secrets** after revoking a viewer — `rotate_file` / `rotate_folder` re-keys the published entry so the old secret stops working.

## Code locations

| Concern | Location |
|---|---|
| Keys (Ed25519, X25519) | `crates/zim-crypto/src/keys/` |
| Content encryption (ChaCha20-Poly1305) | `crates/zim-crypto/src/secret.rs` |
| Secret sharing (ECDH + AES-KW) | `crates/zim-crypto/src/secret_share.rs` |
| Manifest (shares, relays, published) | `crates/zim-core/src/fs/manifest.rs` |
| Share + dialable flag | `crates/zim-core/src/fs/share.rs` |
| Hub identity vault | `crates/zim-hub/src/identity/` |
| WASM key unlock | `crates/zim-wasm/src/lib.rs` |
| Sync protocol | `crates/zim-protocol/src/peer/` |

## Dependencies

| Crate | Purpose |
|---|---|
| `iroh` | P2P networking, QUIC transport, blob storage |
| `ed25519-dalek` | Identity keypairs |
| `chacha20poly1305` | Content + vault encryption |
| `aes-kw` | Key wrapping (RFC 3394) |
| `blake3` | Content addressing (via iroh) |
| `argon2` | Password-derived KEK for hub vault |
| `serde_ipld_dagcbor` | DAG-CBOR serialization |

## Related concepts

- [Access Model](./access-model.md) — owner/relay/web-key roles, publication, hub as Mirror + Relay.
- [Identity](./identity.md) — vault-not-custodian pattern, Argon2id, device model, threat model detail.
- [Cryptography](./cryptography.md) — primitive specifications.
- [Data Model](./data-model.md) — manifest schema, `Share`, `Relay`, `Published`.
