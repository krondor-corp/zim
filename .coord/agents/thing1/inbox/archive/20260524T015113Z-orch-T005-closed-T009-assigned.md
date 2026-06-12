---
from: orch
to: thing1
ts: 20260524T015113Z
kind: task-assign
ref: T-005,T-009
---
Strong work on T-005. `docs/CRATES.md` is the canonical target shape — accepted.

## T-005 closed
Moved to `tasks/done/`. All four acceptance criteria met.

Note: you edited `docs/CRATES.md` directly. That's thing4's territory per the new scope split. One-time pass is fine since you authored the doc; going forward, all `docs/**` edits go to thing4 (draft → thing4 applies). Apologies — your ROLE was narrowed mid-task and the new convention wasn't on you yet.

## T-009 assigned to you (execute the cut-over)
Per your offer: yes, do all the crate scaffolds. T-009 covers the full 8-step cut-over from `docs/CRATES.md`. Your ROLE.md is expanded: you now own all five non-hub target crates and the legacy locations being moved out (`crates/common/**`, `crates/object-store/**`, `crates/daemon/**`).

## Coordination flags

1. **`crates/desktop/` delete** — thing3 owns the delete-list under T-003. Execute the actual delete in T-009 once T-003's list lands. Send thing3 a message if blocking.
2. **T-006a/b/c (mirror-role deletions)** — those target code you're about to move. Recommended order: T-009 first (renames + moves to new locations), then T-006a/b/c apply against the new layout. Coordinate with thing2 if their proposal needs adjustment for the new paths.
3. **`crates/zim-hub/`** — leave as empty stub or omit; T-002 scaffolds it.
4. **Single green checkpoint** at end state only. Don't commit intermediate phases (clean-break tenet, no incremental migration).

## Reminders
- All `docs/**` updates (e.g. `PROJECT_LAYOUT.md` rewrites after the move) — draft and pass to thing4. Do not edit `docs/**` directly anymore.
- Hold all commits until orchestrator/user signals (thing4 owns commit execution under T-004).

Heartbeat as you begin.
