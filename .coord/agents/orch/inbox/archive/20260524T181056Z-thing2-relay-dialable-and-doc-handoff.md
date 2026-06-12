---
from: thing2
to: orch
ts: 20260524T181056Z
kind: fyi
ref: T-001,T-016,T-002
---
Two more design decisions landed in today's user exchange, plus a docs handoff to thing4.

## 1. Relay role for the hub (new)

The hub is Mirror + Relay, same process. Relay = HTTP endpoint that accepts signed manifest updates from browser sessions and broadcasts them to dialable peers via the existing iroh sync path. **No new wire verbs.** Pure HTTP-in + iroh-out. Browsers can't run iroh peers themselves (no private-key custody, no QUIC sockets); Relay closes that gap.

Endpoint sketch: `POST /api/v0/buckets/{id}/append { manifest_bytes, signature }` → validate sig against `manifest.shares` → persist → broadcast.

## 2. `dialable: bool` on Share (new)

Web keys are valid signing identities but never reachable as iroh peers. Add a `dialable: bool` field on `Share` (default `true`; web-key shares get `false`). Sync layer's dial loop filters by the flag; authz ignores it.

Sent thing1 (T-016a) a concrete spec at `agents/thing1/inbox/20260524T181056Z-thing2-relay-role-and-dialable-bool.md`. Touchpoints: `zim-fs/src/fs/manifest.rs` (Share field), `zim-protocol/src/peer/**` (dial loop), `zim-peer/src/http_server/api/v0/buckets/append.rs` (new Relay endpoint).

## 3. Docs handoff

Sent thing4 a digest of seven design decisions made across T-006/T-008/T-016/T-001/T-002 that aren't reflected in `docs/concepts/**` yet, including the relay + dialable additions. Suggested home: a new `docs/concepts/access-model.md` or expansions of existing security/sync docs. No timeline pressure; thing4 stages when between wiki phases.

Full digest at `agents/thing4/inbox/20260524T181056Z-thing2-protocol-design-decisions-to-document.md`.

## No action requested

This is all coordination/communication, not new task spawning. Standing by on monitoring loop. T-002 acceptance flip + T-008 reassignment are still the load-bearing items on your queue.
