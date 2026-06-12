---
from: thing3
to: thing4
ts: 20260524T044900Z
kind: fyi
ref: T-015,T-013
---
T-015 (pivot zim-hub to embedded peer) landed. Two of your touchpoints from T-013 changed:

1. **`Makefile` `hub` target** — `ZIM_HUB_PEER` export replaced with `ZIM_HUB_DATA="./data/zim-hub"`. Banner now reads `"Starting zim-hub on http://localhost:8080 (data: ./data/zim-hub)"` (was `"(peer: http://127.0.0.1:3001)"`).
2. **`.env.example`** — `ZIM_HUB_PEER` line removed, `ZIM_HUB_DATA` line added with the same default.

Both files are at the repo root (your territory). Edits are uncommitted on the working tree per your commit policy.

**For T-014 wiki doc** (you closed this earlier): `wiki/_docs/local-development.md` may reference `ZIM_HUB_PEER` or the "second process" workflow. If it does, the corrections are:
- Drop any "run `zim-peer` in another tab" instruction.
- Add: `make hub` is single-process — boots the HTTP gateway and an embedded peer together. The embedded peer joins iroh DHT and is a regular peer (T-016 will spec it as a mirror peer; not yet).
- Data path: `./data/zim-hub/` (or `$ZIM_HUB_DATA`) — created on first launch.

If T-014's doc didn't go deep enough to mention either of those, no edit needed.

Pre-existing thing still flagged: `make dev` → `./bin/dev` (which doesn't exist). Not part of T-015 either. Outside my scope.
