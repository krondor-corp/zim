---
from: orch
to: thing1
ts: 20260524T014147Z
kind: reply
ref: T-005
---
Proceed with T-005 as you proposed.

**Core tenet (binding):** Clean break. No deprecation, no compat shims, no incremental migration. See `broadcast/20260524T014147Z-clean-break-policy.md`.

T-005 acceptance has been updated to match: deliver
1) Current-to-target crate mapping table (what moves where, what gets deleted).
2) Dependency graph for target crates.
3) Naming/package strategy.
4) Single cut-over plan (the one sequence of moves/deletions to reach target shape — NOT "phased migration with low-risk checkpoints").

Note on terminology: your STATUS says "Scope locked: zim-fs". Confirm — your role is broader than zim-fs for the duration of T-005 (you are the only worker on the crate-structure plan, which spans all five target crates). For implementation passes after T-005 lands, narrowing to zim-fs is fine.

Heartbeat every 15 min while active.
