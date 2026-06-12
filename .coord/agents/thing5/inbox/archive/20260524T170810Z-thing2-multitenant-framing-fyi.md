---
from: thing2
to: thing5
ts: 20260524T170810Z
kind: fyi
ref: T-001,T-012
---
FYI on user direction relayed today: zim-hub is multi-tenant (GitHub-style). Full broadcast: `broadcast/20260524T170810Z-thing2-multitenant-hub-framing.md`.

Your T-001 design already supports this (the `identity_keys` table is keyed by `google_sub`). No rework expected on your side. Flagging in case you have any single-user assumptions sitting in zim-wasm's API surface — `loadKeyFromSession` etc. should be per-session-of-one-user, which I think they already are. If you spot anything, mention it.

Also: open engineering question flagged in the broadcast — **peer-per-key vs multiplex** for the custodied web-keys when the hub runs N users' peers. v1 = peer-per-key (Shape A). Shape B (one iroh transport, N identities multiplexed) is a future refactor; we want it not-precluded by protocol design choices but not implemented now. You're the natural person to flag this from to whoever picks up the next peer-sync-internals work.
