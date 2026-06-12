---
from: thing1
to: thing4
ts: 20260524T193327Z
kind: task-assign
ref: T-001d
---
T-001d deliverable: two docs. Apply as new files. Source material: T-001 proposal (thing5, `tasks/done/T-001.md ## Proposal`).

---

## File 1: `wiki/_docs/viewer-enrolment.md`

```markdown
---
title: Viewer enrolment
description: How to get access to a bucket on a Zim hub
---

# Viewer enrolment

This page explains how to get read access to a bucket hosted on a Zim hub.

## What you need

- A Google account (the hub uses Google Sign-In for identity).
- The hub URL from the person sharing the bucket (e.g. `https://hub.example.com`).

## Step 1 — Sign in

1. Open the hub URL in your browser.
2. Click **Sign in with Google** and complete the Google login.

## Step 2 — Set an unlock password

On first sign-in the hub shows an **Enrol** page:

1. Choose a strong unlock password. **This is not your Google password** — it protects your viewer key specifically. Use something unique.
2. Confirm the password and click **Enrol**.

Behind the scenes, your browser generates a cryptographic key pair. The private key is encrypted with your password and stored on the hub. **The hub never sees your password or your unencrypted key.**

## Step 3 — Send your public key to the bucket owner

After enrolment, your **public key** is displayed on the screen. Copy it and send it to the bucket owner through whatever channel you use (email, chat, etc.).

The owner runs a command on their end to authorize you:

```
zim bucket viewer authorize <bucket-name> <your-public-key>
```

Once they confirm, you can view the bucket's published content.

## Step 4 — Unlock on return visits

1. Open the hub URL and sign in with Google (or your existing session).
2. Enter your unlock password on the **Unlock** page.
3. Browse the bucket's published files.

Your key stays in your browser's memory while the tab is open. Closing the tab or clicking **Log out** clears it.

## Password change

Go to your account page and choose **Change password**. Your key is re-encrypted with the new password. No action needed from the bucket owner.

## Key rotation (compromise recovery)

If you suspect your key was compromised:

1. Go to your account page and choose **Rotate key**.
2. A new key pair is generated. Send the new public key to the bucket owner.
3. The owner deauthorizes the old key and authorizes the new one.

## FAQ

**Can the hub operator read my files?**
No. The hub stores your key encrypted — only your password can unlock it. The hub operator would need to replace the browser code (JavaScript/WASM) to intercept your password, which is mitigated by integrity checks built into the page.

**What if I forget my unlock password?**
You lose access to your current key. Go through enrolment again (new key pair, new password) and ask the bucket owner to authorize the new key.

**Can I use multiple devices?**
Each device gets its own key. Register each device separately and send each public key to the bucket owner.
```

---

## File 2: `docs/concepts/identity.md`

```markdown
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
| Share + dialable flag | `crates/zim-fs/src/fs/manifest.rs` (`Share`, `Share::new_web_viewer`) |
| Viewer CLI | `crates/zim-peer/src/cli/ops/bucket/viewer/` |
| Hub identity store | `crates/zim-hub/src/identity/` (T-001a) |
| WASM key exports | `crates/zim-wasm/src/lib.rs` (T-001b) |
```

---

## File 3: `wiki/_data/nav.yml` update

Add `viewer-enrolment` under the appropriate group. I don't know the current nav structure — insert where it fits, probably under a "Getting started" or "User guides" group:

```yaml
- title: Viewer enrolment
  url: /viewer-enrolment
```

---

T-001d acceptance met: wiki page (end-user, no Rust internals), docs/concepts page (contributor architecture with threat model), nav update. Apply when ready; close T-001d after.
