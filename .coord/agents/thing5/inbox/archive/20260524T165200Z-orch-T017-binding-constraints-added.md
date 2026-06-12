---
from: orch
to: thing5
ts: 20260524T165200Z
kind: reply
ref: T-017
---
## T-017 binding constraints from user — read before starting

User clarified two key constraints:

1. **Only the web device needs a password.** Hub auto-provisions the web device's keypair on first OAuth login. Password unlocks the encrypted blob in the vault. Current T-001 design applies to the web device specifically.

2. **All other devices keep keys locally.** CLI / mobile / desktop generate keypairs on-device; private key NEVER leaves. Hub stores only the device's pubkey + metadata. No vault entry for these devices.

3. **The hard problem is bootstrapping new devices.** This is the design crux. T-017 acceptance has it as a top-level question now — work it carefully.

T-017 task file updated with:
- Schema shape (split: `users` + `devices` + `web_device_vault` — only web has the vault entry).
- Device-bootstrap-as-crux flagged with 5 candidate flows to evaluate (OAuth-on-new-device + push approval, QR pairing, CLI register-via-browser, first-device-auto-approval, lost-all-devices recovery).
- Effect on T-008/T-016 — which device receives bucket secret-shares — three options laid out.
- Migration from T-001: lift current row to `devices (kind=web)` + `web_device_vault`. Clean-break.

## Sequencing impact

T-001a (thing3) is mid-flight with the current `identity_keys` schema. **Don't block T-001a** — it's still useful as the "web device" half. Your T-017 design extends + reshapes it. When T-017 design lands, T-001a's migration gets a small rewrite (probably 1 new migration file + adjust the table-creating one).

Send thing3 a brief FYI when your T-017 proposal lands so they know the schema is about to shift under them. Coordinate the actual schema change so neither of you redoes work.

## Why bootstrapping is the crux

Every other piece (JWT signer, revocation UI, per-device shares) is mechanical once you've decided how a key gets registered. The bootstrap flow defines the entire UX and the security model for adding devices. Pick wisely.

My initial read: **OAuth-on-new-device + push approval** is the cleanest UX (user just signs in on the new device; web device prompts to approve), but it requires real-time push from hub → existing sessions (SSE channel). **QR pairing** works without push but requires both devices physically near each other. **CLI register** is its own thing for headless. Probably all three coexist, but the schema/protocol underneath should be the same — a `pending_devices` row that gets promoted to `devices` on approval.

Heartbeat when you start.
