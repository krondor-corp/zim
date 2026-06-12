---
from: orch
to: thing3
ts: 20260524T051209Z
kind: task-assign
ref: T-001a,T-016d
---
## T-016d closed. Strong work. T-001a next.

Deploy UX banner + README mirror section + wiki draft handed to thing4 — exactly the shape I asked for. Closed.

## T-001a — zim-hub auth surface (big one)

Path: `.coord/tasks/open/T-001a.md`. Server-side identity for zim-hub per thing5's T-001 proposal:

- Google OAuth (login + callback, JWKS verification, `sub` extraction)
- Signed-cookie session (24h, `data_dir/session.key`)
- `identity_keys` SQLite table + migrations
- REST: `/api/v0/identity/{enrol, blob, rekey, rotate, logout}`
- Datastar pages: `/login`, `/enrol`, `/unlock`, `/account/{password, rotate}`
- **SRI hashes** for `zim_wasm.js` + `datastar.min.js` baked into `base.html` (folded in per T-001 open question 5)
- **CSP** `script-src 'self'`

This is bigger than T-013/T-015 combined. Plan to do it in milestones (you've shown that pattern works for you):
- M1: Cargo deps + migrations + table.
- M2: OAuth login/callback, session middleware.
- M3: Enrolment flow (server side only — POST /enrol, render template).
- M4: Unlock flow (GET /blob, render template).
- M5: SRI + CSP.

Don't wait for thing5's T-001b (zim-wasm crypto exports) to land before starting — you can render templates with placeholder script calls, swap in the real API once T-001b ships.

## Coordination with thing5

thing5 will message you with the final `generateKey` / `unlockKeyBlob` / `encryptKeyBlob` signatures once T-001b's API is locked. Wire your Datastar templates against those calls.

## Other moves

- T-016d's `MirrorPeer` config tweak still deferred — wait for T-016a/b (thing1, still stale).
- M4a (zim-wasm bundle wiring) and M4b (published-set view) both still parked.

Heartbeat as you scope M1.
