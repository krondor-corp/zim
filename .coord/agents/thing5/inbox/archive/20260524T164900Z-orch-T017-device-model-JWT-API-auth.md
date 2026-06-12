---
from: orch
to: thing5
ts: 20260524T164900Z
kind: task-assign
ref: T-017,T-001,T-008a
---
## T-017 assigned: device model + ed25519-signed JWT API auth

User flagged two gaps:
1. No JWT signer using user's ed25519 key for zim-hub API auth.
2. No device-management concept (one Google identity = one key today; should be one identity = N devices).

These are coupled — a device model naturally produces JWT signers (each device's key signs its own JWTs).

Path: `.coord/tasks/claimed/T-017.md`. Design-only proposal first, same shape as T-001/T-008/T-016. Sub-tasks for impl spawn after.

## Key questions you need to answer

- Schema reshape: split `identity_keys` into `users` + `devices`, or stay flat with composite PK?
- Per-device password vs shared? (Recommend per-device for strongest security.)
- Device registration flow for adding a second device (existing-device-approves vs OAuth re-flow).
- Effect on T-008/T-016: which device key receives bucket secret-shares?
- JWT algorithm: EdDSA (RFC 8037, matches your ed25519 key type).
- JWT lifetime + refresh: short tokens? Medium? Refresh JWT?
- Where signing happens: zim-wasm for browser; new `zim` CLI subcommand for headless?
- Migration from current T-001 design: treat existing key as "initial device" or wipe? (Per clean-break, your call.)

## Sequencing vs your other work

- T-008a is in flight — keep going. T-017 is design, you can do both.
- When T-017 sub-tasks spawn, T-017a (schema) interacts with T-001a (thing3's auth surface) — coordinate the migration with thing3.
- T-017's "which device key receives bucket shares" question affects T-008 — fold the answer into your T-008a impl if T-017 design lands fast.

## Why now

The current T-001 design promises "one Google identity = one key" — limits the product to "one browser session per user forever." Real users want phone + laptop + maybe a CLI. The user is asking now because they're noticing the gap before it's load-bearing.

Heartbeat as you start.
