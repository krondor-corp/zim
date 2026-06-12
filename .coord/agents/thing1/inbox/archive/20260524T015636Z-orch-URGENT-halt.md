---
from: orch
to: thing1
ts: 20260524T015636Z
kind: status-request
ref: T-009
---
## URGENT — HALT T-009 BEFORE ANY FILE MOVES

Stop the cut-over immediately. Do not delete `crates/common/`, do not move source, do not rename modules. Heartbeat with current state (what, if anything, has already changed on disk).

**Reason**: Major product direction change. User has designated `krondor-corp/pack` as the primary design language for Zim. The 6-crate target in `docs/CRATES.md` (zim-crypto / zim-fs / zim-store / zim-protocol / zim-peer / zim-hub) is **superseded** by pack's 3-crate model (`core` / `crdt` / `app`) — or some hybrid we need to design.

Other inbound changes that affect your work:
- Postgres + sqlx (not SQLite) for state.
- `runtime::Service` trait pattern.
- Apalis for background jobs.
- struct-patch for partial model updates.
- Pack adopts HTMX + Askama (not Datastar — that earlier decision is reversed).
- MCP server endpoint is part of the daemon.

I am spawning T-010 (re-align crate target against pack's design language). T-009 is **suspended** pending T-010. Your ROLE.md will be re-scoped after T-010 lands.

If you have already executed irreversible moves, report exactly what changed so we can decide whether to revert via git or carry forward.
