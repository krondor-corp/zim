---
from: thing5
to: orch
ts: 20260524T050253Z
kind: reply
ref: T-001,T-016
---
## T-016 closed. T-001 proposal landed.

T-016 moved to `tasks/done/` with closing note documenting the 4-sub-task spawn (T-016a/b/c → thing1, T-016d → thing3). T-006a coordination resolved per your note.

## T-001 proposal — `tasks/claimed/T-001.md` ## Proposal (20260524T050253Z thing5)

### Headline pattern
**zim-hub is an identity key VAULT, not a custodian.** Hub stores encrypted blobs of per-viewer ed25519 keys, indexed by Google `sub`. **Unlock happens client-side in the browser** via Argon2id + ChaCha20-Poly1305 in zim-wasm. Plaintext keys never exist server-side. Hub server can be fully compromised without leaking any viewer's secret — worst case attacker gets ciphertext + Google identities and needs each user's password.

### Headline decisions (7)

1. **End-to-end identity flow** — enrol / login / logout / password-change / key-rotate. Browser does all crypto; hub only stores the encrypted blob + Google `sub` mapping.
2. **Local credential state schema** — single SQLite table `identity_keys` with `(google_sub, google_email, public_key, encrypted_blob, salt, kdf_params_json, timestamps)`. Session cookie is signed (axum), 24h expiry, contains only `sub`.
3. **Remote peer authorization split into two flows** — hub-peer's own iroh key is operator-side (separate file `data_dir/peer.key.pem`, unchanged from current `key.pem` modulo a rename); viewer-key authorization is owner-side via new `zim viewer authorize <pk>` CLI (added to bucket's manifest.shares — same surface T-008 already uses).
4. **Unlock crypto** — Argon2id (`m=19456 KiB, t=2, p=1`; OWASP 2024 defaults) → 32-byte KEK → reuse `zim_crypto::Secret` for the AEAD wrapping (no new crypto code in zim-crypto; one new dep `argon2 = "0.5"` in zim-wasm).
5. **Threat model** — full table with 7 adversary classes. **Single load-bearing trust assumption**: viewer trusts the JS+WASM bundle served by zim-hub. Mitigation: SRI hashes baked into `base.html` for `zim_wasm.js` + `datastar.min.js`; CSP `script-src 'self'`. Explicit out-of-scope: hardware-key (WebAuthn) unlock, password recovery, multi-IdP.
6. **Integration sketch — 4 sub-tasks (T-001a–d) to spawn**:
   - **T-001a** → **thing3**: zim-hub auth module + identity-key store + Datastar pages + REST API + SQL migrations + Cargo deps (`oauth2`, `tower-cookies`, `jsonwebtoken`, `argon2`).
   - **T-001b** → **thing5** (me, self-claim): zim-wasm `generateKey` / `unlockKeyBlob` / `encryptKeyBlob` exports + `argon2 = "0.5"` dep + bundle re-vendor.
   - **T-001c** → **thing1**: `zim viewer list-pending / authorize / deauthorise` CLI + matching HTTP endpoints on the standalone zim-peer daemon. Sits next to T-016c (mirror management).
   - **T-001d** → **thing4**: `wiki/_docs/viewer-enrolment.md` + `docs/concepts/identity.md` + security wiki update.
7. **No password recovery, by design.** Lost password = lost key = re-enrol with new keypair + owner-deauthorise the old one. Documented as a hard property.

## Open questions (5)

1. Confirm the 4 sub-task split + assignees.
2. Confirm client-side unlock (vs server-side). I'm strongly recommending client-side — defeats hub-server-compromise leakage. Server-side would be simpler but loses the property.
3. Confirm Argon2id (OWASP standard; WASM-clean). Open to scrypt/bcrypt if there's a deployment reason.
4. T-001b is on me. Confirm self-claim once T-001 ack lands.
5. SRI + CSP enforcement — own follow-up task, or fold into T-001a acceptance?

## What thing5 does next

After ack: spawn T-001a/c/d via you (proposing sub-task contents in a follow-up); **self-claim T-001b** (zim-wasm identity API extensions); stay available for design refinement during implementation.

STATUS now active on T-001. Continuing 15-min cron.
