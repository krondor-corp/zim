# Identity and key management

Zim's identity model follows a **vault-not-custodian** pattern: the hub stores encrypted blobs of viewer private keys, but never holds the plaintext. Unlock happens client-side in the browser via zim-wasm.

## Architecture

### Viewer identity

Each viewer has an Ed25519 key pair. The key pair is created in the browser (zim-wasm) and the private key is immediately encrypted before leaving WASM linear memory:

1. **Key derivation**: Argon2id (m_cost=19456 KiB, t_cost=2, p_cost=1) derives a 32-byte KEK from the viewer's chosen password + a random 16-byte salt.
2. **Encryption**: ChaCha20-Poly1305 (same AEAD as bucket content encryption) wraps the Ed25519 secret key bytes with the KEK. 12-byte nonce prepended per `zim_crypto::Secret` wire format.
3. **Storage**: the encrypted blob, salt, KDF params (JSON), and the public key (hex) are stored in the hub's `identity_keys` SQLite table, keyed by Google `sub` claim.

The hub server can be fully compromised without leaking viewer secrets — the attacker gets ciphertext + Google identities but needs each user's password to decrypt.

### Hub peer identity

The hub's embedded peer has its own iroh `PrivateKey` at `data_dir/peer.key.pem`, generated on first start. This is the hub's network identity (operator-side), unrelated to viewer keys. The bucket owner adds this key to `manifest.mirrors` to authorize the hub as a Mirror peer.

### Device model (T-017)

Each physical device (browser tab, CLI, mobile) gets its own Ed25519 key pair:

- **Web device**: key pair in hub-side encrypted vault (Argon2id + ChaCha20 as above).
- **CLI / native device**: key pair generated on-device, private key never leaves. Hub stores only the public key + metadata.

The `Share::dialable` field distinguishes network-reachable keys (`dialable: true`, native devices) from browser-resident keys (`dialable: false`, web devices). The sync dial loop skips non-dialable shares.

## Trust model

| Adversary | Mitigation |
|---|---|
| Compromised hub server | Cannot decrypt viewer keys without per-user passwords. Cannot impersonate viewers in protocol (key never leaves browser). **Critical residual**: attacker can swap the served JS/WASM bundle to phish passwords. Mitigated by SRI hashes on script tags + CSP `script-src 'self'`. |
| Compromised Google account | Attacker can log into zim-hub as victim but still needs the unlock password (second factor). |
| Hub operator | Same as compromised server — cannot decrypt without passwords; can phish via swapped JS. SRI + CSP mitigate. |
| Brute force on stolen blob | Argon2id with 19 MB memory cost. Recommend ≥16-char passwords. |

**The single load-bearing trust assumption**: the viewer trusts the JS+WASM bundle served by the hub. Mitigation: SRI hashes baked into HTML templates, CSP `script-src 'self'`, vendored/version-pinned JS bundles.

## Key flows

### Enrolment (first time)

1. Google OAuth2 PKCE → hub gets `id_token`, extracts `sub` + `email`.
2. Hub establishes signed-cookie session.
3. zim-wasm `generateKey()` → Ed25519 key pair in WASM memory.
4. Viewer chooses password → zim-wasm `encryptKeyBlob(password)` → `{encrypted_blob, salt, public_key}`.
5. Browser POSTs to hub `/api/v0/identity/enrol`.
6. Viewer sends public key to bucket owner out-of-band.
7. Owner runs `zim bucket viewer authorize <bucket> <pubkey>` (or `--web-key` for browser keys).

### Login (returning)

1. Google OAuth (or existing session).
2. Hub serves encrypted blob → browser GETs `/api/v0/identity/blob`.
3. zim-wasm `unlockKeyBlob(blob, salt, password)` → Argon2id-derives KEK → ChaCha20-decrypts → Ed25519 secret in `SESSION_KEY` thread-local.
4. `decryptBlob` calls work for sealed envelopes targeted at this viewer.

### Deauthorization

Owner runs `zim bucket viewer deauthorise <bucket> <pubkey>`. Share removed from manifest. **Known gap (T-001c-followup)**: per-node secrets are not yet re-keyed; cached blobs remain decryptable by the deauthorized viewer until secret rotation ships.

## Relevant code

| Concern | Location |
|---|---|
| Key types | `crates/zim-crypto/src/keys/{private,public}.rs` |
| Secret encryption | `crates/zim-crypto/src/secret.rs` (ChaCha20-Poly1305) |
| Secret sharing (ECDH) | `crates/zim-crypto/src/secret_share.rs` |
| Share + dialable flag | `crates/zim-core/src/fs/manifest.rs` (`Share`, `Share::new_web_viewer`) |
| Viewer CLI | `crates/zim-peer/src/cli/ops/bucket/viewer/` |
| Hub identity store | `crates/zim-hub/src/identity/` (T-001a) |
| WASM key exports | `crates/zim-wasm/src/lib.rs` (T-001b) |
