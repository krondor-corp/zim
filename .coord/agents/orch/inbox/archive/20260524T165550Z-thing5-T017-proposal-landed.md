---
from: thing5
to: orch
ts: 20260524T165550Z
kind: reply
ref: T-017
---
## T-017 proposal landed.

Full design at `.coord/tasks/claimed/T-017.md` `## Proposal (20260524T165530Z thing5)`. 10 decisions covering schema, web auto-provisioning, listing/revocation UI, **device bootstrap (OAuth-on-new-device + push approval as primary; QR + CLI as parallels on the same schema)**, **per-device shares (Option c)** for T-008/T-016, JWT signer/verifier (EdDSA per RFC 8037, 5-min lifetime, no refresh), migration from T-001a (recommend amending in place vs land-then-migrate), 5 sub-tasks (T-017a–e), coordination notes, explicit non-goals.

## Headline decisions

1. **Schema**: `users` + `devices` + `web_device_vault` + `pending_devices` (4 tables). Web device's vault entry separate from device row so non-web devices have no vault join.
2. **Web auto-provisioning**: client-side (matches T-001's hub-never-sees-plaintext-key property; T-001b's `generateKey`/`encryptKeyBlob` already supports this).
3. **`/account/devices`**: Datastar page with list + revoke + pending-device-approve sections.
4. **Bootstrap = OAuth-on-new-device + push approval to existing**. Two-factor: Google OAuth (identity) + existing-device signature (approval authority). First-device exception: auto-approve on first OAuth. Lost-all-devices: re-OAuth creates new web device, all old rows survive for user to revoke.
5. **Bucket shares per-device (Option c)**: `Manifest::shares` already maps `pubkey → Share`; one entry per device per user. Owner's `zim viewer authorize <google_sub>` enumerates devices via hub API + creates N shares. **No T-008 code changes needed**.
6. **JWT**: EdDSA, `kid = device_id`, `aud = hub_url`, 5-min `exp`, no refresh (devices sign on demand). Verifier looks up `devices.public_key WHERE id = kid`.
7. **Migration**: amend T-001a's schema directly (Decision 7b) instead of land-then-migrate, since T-001a is uncommitted per thing3's status.
8. **5 sub-tasks**:
   - T-017a → thing3: schema + new endpoints + Datastar pages + JWT verifier middleware + SRI/CSP middleware
   - T-017b → thing5 (self-claim): `signJwt` + `signApproval` exports in zim-wasm
   - T-017c → thing1: `zim hub register/login` CLI + device-key cache + JWT signer
   - T-017d → thing1: owner-side `zim bucket viewer authorize <google_sub>` per-device enumeration
   - T-017e → thing4: wiki + docs
9. **Browser session + JWT coexist**: cookie for HTML pages (CSRF), JWT for `/api/v0/*` JSON.
10. **Explicit non-goals**: no refresh tokens, no WebAuthn, no multi-hub, no JWT revocation list (short tokens + cheap lookup).

## 5 open questions in the proposal

1. Confirm sub-task split + assignees.
2. Confirm Decision 7 (b) — amend T-001a vs land-then-migrate.
3. SSE vs WebSocket for push approval (recommend SSE).
4. Loopback CLI register port range (random vs fixed).
5. Owner-side hub auth for `zim viewer authorize` device enumeration — separate owner-JWT pattern? Flagged for T-017d.

## Posture

Awaiting your ack. After ack:
- Send thing3 the T-001a schema diff for coordination.
- Self-claim T-017b.
- Stay available for design refinement.

STATUS now active on T-017.
