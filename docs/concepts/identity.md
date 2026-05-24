# Identity & Key Custody

How Zim handles viewer identity for the `zim-hub` read-only mirror gateway: who a viewer is, how their key material gets onto the hub, and how it stays out of the hub server's hands.

Source design: T-001 proposal (thing5) in `.coord/tasks/done/T-001.md`. This doc states the shipped architecture; sub-tasks T-001a/b/c/d implement it.

## Headline: hub is a vault, not a custodian

`zim-hub` stores **encrypted blobs of per-viewer private keys**, indexed by Google identity. Unlock happens **client-side in the browser** (via `zim-wasm` + Argon2id). Plaintext viewer keys never exist server-side.

The trust property that follows:

- A fully compromised hub server (root + DB + TLS) leaks ciphertext + Google identities. It does not leak any viewer's private key.
- The hub operator cannot impersonate viewers in the protocol — viewer keys never leave the browser as plaintext.
- Pair with [T-006 + T-016](./security.md): hub-as-mirror-peer never holds any **bucket** secret. T-001 adds: hub-as-key-vault never holds any **viewer** secret either.

The single load-bearing trust assumption is **the JS+WASM bundle the hub serves**. The threat model section below explains the mitigation (SRI + CSP).

## Two-layer identity

| Layer | What it identifies | Lifetime | Storage |
|---|---|---|---|
| **Hub-peer identity** | The hub's own iroh peer key (network identity) | One per hub deployment | `data_dir/peer.key.pem` — operator-side, plaintext |
| **Viewer identity** | Per-viewer ed25519 keypair (bucket-membership identity) | One per viewer per hub | Encrypted blob in hub's SQLite; plaintext only in browser memory |

Hub-peer identity is the same shape as any `zim-peer` deployment — it's how the hub joins iroh DHT. Bucket owners authorise it via `zim mirror add <hub-pk>` (per T-016c) so the hub can sync as a Mirror peer-type (per T-016 Decision 1).

Viewer identity is new. It is the subject of the rest of this doc.

## Enrolment flow (first-time viewer)

```
viewer browser                  zim-hub                     bucket owner peer
      |                            |                                |
      |  GET /                     |                                |
      |--------------------------->|                                |
      |  302 → Google OAuth        |                                |
      |<---------------------------|                                |
      |                                                             |
      |  ... Google PKCE flow ...                                   |
      |                                                             |
      |  GET /callback?code=...    |                                |
      |--------------------------->|                                |
      |                            | verify id_token, extract `sub` |
      |                            | lookup identity_keys by `sub`  |
      |                            | NOT FOUND → render /enrol      |
      |  enrolment page            |                                |
      |<---------------------------|                                |
      |                                                             |
      |  zim_wasm.generateKey() → (pk, sk in WASM memory)           |
      |  prompt: new password                                       |
      |  zim_wasm.encryptKeyBlob(password)                          |
      |    → Argon2id(password, salt) = KEK                         |
      |    → ChaCha20-Poly1305(KEK, sk) = encrypted_blob            |
      |    → returns (encrypted_blob, salt, kdf_params, pk)         |
      |                                                             |
      |  POST /api/v0/identity/enrol                                |
      |    body: encrypted_blob, salt, kdf_params, pk               |
      |--------------------------->|                                |
      |                            | INSERT identity_keys           |
      |  200 + display pk          |                                |
      |<---------------------------|                                |
      |                                                             |
      |   ... viewer sends pk to owner out-of-band ...              |
      |                                                             |
      |                                            zim viewer authorize <pk> --bucket <id>
      |                                            (T-001c CLI on owner peer)
      |                                                             |
      |                                            appends manifest entry,
      |                                            seals bucket Secret share for pk
      |                                                             |
      |  next login: zim_wasm.unlockKeyBlob → SecretShare decryption works
```

Key properties of this flow:

- Browser is the only party that ever sees `sk` (the viewer's ed25519 secret key) in plaintext.
- Password is the only party that ever sees the password.
- The hub sees: Google `sub`, Google `email`, `pk`, `encrypted_blob`, `salt`, `kdf_params`. Enough to recognise returning viewers; not enough to decrypt anything.

## Login flow (returning viewer)

```
viewer browser                  zim-hub
      |  GET /                            |
      |---------------------------------->|
      |  (cookie? — yes/no)               |
      |  ... OAuth if needed ...          |
      |  resolve `sub`                    |
      |  lookup identity_keys → FOUND     |
      |  /unlock page                     |
      |<----------------------------------|
      |                                   |
      |  GET /api/v0/identity/blob        |
      |---------------------------------->|
      |  {encrypted_blob, salt,           |
      |   kdf_params}                     |
      |<----------------------------------|
      |                                   |
      |  password prompt                  |
      |  zim_wasm.unlockKeyBlob(blob, salt, password, kdf_params)
      |    Argon2id(password, salt) = KEK
      |    ChaCha20-Poly1305.decrypt(KEK, blob) → sk in WASM mem
      |    on AEAD-tag failure: re-render with "wrong password"
      |                                   |
      |  unlocked — decryptBlob() now works on any bucket blob whose
      |  SecretShare was sealed for this viewer's pk.
```

## Local credential state (hub-side schema)

```sql
CREATE TABLE identity_keys (
    google_sub      TEXT    PRIMARY KEY,
    google_email    TEXT    NOT NULL,
    public_key      TEXT    NOT NULL,
    encrypted_blob  BLOB    NOT NULL,
    salt            BLOB    NOT NULL,
    kdf_params      TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,
    last_used_at    INTEGER NOT NULL,
    UNIQUE(public_key)
);

CREATE INDEX idx_identity_keys_public_key ON identity_keys(public_key);
```

Implementation lives in `crates/zim-hub/migrations/` and `crates/zim-hub/src/identity.rs`.

| Column | Notes |
|---|---|
| `google_sub` | Google `sub` claim from the `id_token`. Stable per-account opaque string. The PK. |
| `google_email` | Denormalised from the `id_token` for display + owner-side recognition. Treat as best-effort, not security-bearing — Google can change it. |
| `public_key` | Hex-encoded ed25519 pubkey (32 bytes → 64 hex). UNIQUE — the same pubkey can't be enrolled under two Google accounts on the same hub. |
| `encrypted_blob` | ChaCha20-Poly1305 ciphertext of the ed25519 secret. Uses `zim_crypto::Secret`'s wire format (nonce prepended). |
| `salt` | 16 bytes, random per blob, browser-generated via WebCrypto. Public — stored alongside ciphertext. |
| `kdf_params` | JSON: `{m_cost, t_cost, p_cost, alg: "argon2id", version: 19}`. JSON not a fixed schema so we can upgrade Argon2 parameters without a migration — next password change re-encrypts under new params. |
| `created_at` / `last_used_at` | Unix seconds. `last_used_at` refreshed on each successful unlock; gives the owner a "who's active" signal via the audit endpoint. |

**Notably NOT stored on the hub:**

- Google `id_token` past the OAuth callback (only `sub` + `email` extracted).
- `access_token` / `refresh_token` — auth is one-shot at session start; we don't need ongoing Google access.
- Password (ever — Argon2-derived KEK is transient in WASM linear memory).
- Plaintext ed25519 secret (ever, server-side).

## Session cookie

Signed cookie via `tower-cookies`. Payload: `{sub, exp: now + 24h}`.

Signing key: 32 random bytes, persisted at `data_dir/session.key` (0600). Auto-generated on first start.

Refreshed on each authenticated request; rolling 24-hour expiry.

## Key unlock crypto

| Step | Algorithm | Crate | Notes |
|---|---|---|---|
| Password → 32-byte KEK | **Argon2id** | `argon2 = "0.5"` | Pure Rust, WASM-clean. Initial params: `m_cost = 19456 KiB (~19 MB), t_cost = 2, p_cost = 1`. OWASP-recommended (2024); also Bitwarden's defaults. Stored in `kdf_params`. |
| Blob encryption | **ChaCha20-Poly1305** | `chacha20poly1305 = "0.10"` (via `zim-crypto`) | Reuses `zim_crypto::Secret` wire format: 12-byte nonce + ciphertext + 16-byte AEAD tag. The KEK is wrapped in a `Secret` and `Secret::encrypt` does the rest — no new crypto code in `zim-crypto`. |
| Salt | 16 random bytes via WebCrypto / `getrandom` (`wasm_js` cfg). One per blob, public, stored alongside ciphertext. | | |

Pseudo-Rust in `zim-wasm`:

```rust
let argon2 = Argon2::new(
    argon2::Algorithm::Argon2id,
    argon2::Version::V0x13,
    argon2::Params::new(19456, 2, 1, Some(32)).unwrap(),
);
let mut kek = [0u8; 32];
argon2.hash_password_into(password.as_bytes(), &salt, &mut kek).unwrap();
let kek_as_secret = Secret::from_slice(&kek)?;
let plaintext_secret_key = kek_as_secret.decrypt(&encrypted_blob)?;
```

The "Secret" name is slightly overloaded — it's a per-blob symmetric key in bucket context, and a per-user-derived KEK in identity-vault context. The wire format is the same, which is the property we want.

## Threat model

| Adversary | Capability | Mitigation | Residual risk |
|---|---|---|---|
| **Network observer** (CDN, ISP, hostile WiFi) | TLS metadata only. | TLS 1.3 mandatory; HSTS header set. | Metadata observability is inherent. |
| **Compromised hub server** (full root) | Read DB → encrypted blobs + Google identities. Read TLS keys → MITM future sessions. | Cannot decrypt blobs without per-user passwords. Cannot impersonate viewers — viewer-key never leaves browser. | **The hard one.** Attacker can replace the served JS/WASM with a phishing variant that exfiltrates passwords on unlock. Mitigated by SRI + CSP (see below). |
| **Compromised Google account** | Phisher gets victim's Google session → can log into the hub as victim. | Phisher then sees the encrypted blob — but needs the unlock password. Password is a second factor. | If victim uses the same password for Google and for hub unlock: single-credential compromise. Docs say "use a distinct unlock password." |
| **Hub operator** (legitimate owner, malicious intent) | Can see DB + can swap served JS. | Same as compromised server — cannot decrypt without passwords; can phish via swapped JS. | SRI + CSP as below. Operator-side viewer audit: viewer's own pubkey at `/account` must match the pubkey owner sees via `zim viewer list-pending`; mismatch = swapped key. |
| **Bucket owner** (legitimate, but owner-key compromised) | Can decrypt all bucket content. | Out of scope. Owner compromise = full bucket compromise by design. | T-001 doesn't change the owner-side threat model. |
| **Brute force against stolen blob** | Argon2id with 19 MB memory cost — GPU/ASIC attacks 100–1000× more expensive than scrypt/bcrypt. | Recommend ≥16-char passwords. Rate-limit unlock attempts at `/api/v0/identity/blob` (effective server-side; client-side rate-limit is cheap to bypass). | Weak passwords are weak. |
| **Compromised viewer browser** (extension, keylogger) | Reads in-memory key. | Out of scope — fully-owned browser = game over. | Document: "use a clean browser profile for sensitive buckets." |

### The load-bearing trust assumption: SRI + CSP

**The viewer trusts the JS+WASM bundle served by zim-hub.** A compromised hub can swap the bundle for a phishing variant that exfiltrates the password during `unlockKeyBlob`.

Mitigations baked into the templates:

- **SRI hashes** on `<script>` tags for `zim_wasm.js` and `datastar.min.js`. Bundle hashes are computed at vendor-bump time and baked into `crates/zim-hub/templates/layouts/base.html`. A swapped bundle fails the browser's integrity check.
- **CSP** header `script-src 'self'` — no third-party JS, no inline scripts, no eval.
- **HSTS** — forces HTTPS so the SRI check happens over an authenticated channel.

These are implementation requirements for T-001a, not optional. The threat model assumes them.

Future hardening: separate the JS bundle delivery from hub-rendered HTML so JS changes require a deliberate vendor-bump commit. Already the case for `datastar.min.js`; same pattern for `zim_wasm.js` via the wasm-pack output under `crates/zim-hub/static/vendor/`.

## Explicit non-goals

- **Password recovery.** Lost password = lost key. The owner must `zim viewer deauthorise <old-pk>` and the viewer re-enrols with a new keypair. Documented as a hard property. Escrow / social recovery were rejected: they'd require a custodian (defeats the design) or extra ceremony that doesn't fit the "deploy one binary, sign in via Google" pitch.
- **Multi-IdP.** Google only for v1. Adding Apple / GitHub / Microsoft is a follow-up — parameterise the OAuth provider, no design change.
- **Hardware-key unlock (WebAuthn / FIDO2).** Interesting future direction. Would replace the password-derived KEK with a hardware-attested KEK. Out of v1 scope.
- **Federated hubs sharing identity stores.** Each hub is its own vault.
- **Auto-approval of viewers by Google identity alone.** Even with strong identity verification, the bucket owner is the one who controls the bucket. Owner approval is out-of-band (T-001c CLI).

## Code touchpoints

| Concern | Implementation |
|---|---|
| OAuth + session + identity store | `crates/zim-hub/src/auth/` (T-001a) |
| Datastar pages: login, enrol, unlock, account | `crates/zim-hub/src/http/html/auth/` (T-001a) |
| HTTP API: enrol, blob, rekey, rotate, pending, logout | `crates/zim-hub/src/http/api/v0/identity/` (T-001a) |
| SQLite migration | `crates/zim-hub/migrations/20260524051831_create_identity_keys.up.sql` |
| Identity-store queries | `crates/zim-hub/src/identity.rs` |
| Browser-side crypto: `generateKey` / `encryptKeyBlob` / `unlockKeyBlob` | `crates/zim-wasm/src/lib.rs` (T-001b) |
| Owner CLI: `zim viewer list-pending / authorize / deauthorise` | `crates/zim-peer/src/cli/ops/viewer/` (T-001c) |
| Wiki / end-user guide | `wiki/_docs/viewer-enrolment.md` (T-001d) |

## Related concepts

- [Security](./security.md) — bucket-level threat model and protocol invariants.
- [Cryptography](./cryptography.md) — primitives (`Secret`, `Share`, ChaCha20-Poly1305, X25519, BLAKE3).
- [Data Model](./data-model.md) — `Manifest.shares`, per-blob `SecretShare`.
- [Synchronization](./synchronization.md) — mirror peer-type in the sync protocol.
