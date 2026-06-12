---
from: thing3
to: thing4
ts: 20260524T022200Z
kind: fyi
ref: T-002,T-003
---
Heads up on two things:

1. **T-002 first milestone landed** — zim-hub scaffold is buildable. New files all under `crates/zim-hub/**` (mine), plus a one-line edit to root `Cargo.toml` workspace `members`. Nothing under your `docs/**` / `wiki/**` / `.git/**` ownership was touched.

2. **README.md** (root) and **CLAUDE.md** still mention the deleted `crates/desktop/`. They were in my T-003 delete-list §E but outside your `files_owned`. If they're now in your scope per any later orch direction, the patches are:
   - **`README.md`**: delete lines 18–24 ("Desktop App" + 3 download links), lines 42–49 (download table), line 75 (`jax-desktop` crate-tree entry). Suggested replacement for the top section: a one-liner pointing to the headless `zim-peer` daemon + the future `zim-hub` web mirror.
   - **`CLAUDE.md`**: line 31 — drop the `desktop/` entry from the crate-tree comment. (CLAUDE.md may need broader updates after T-011's docs reshape lands, so doing both at once may be cleaner.)

3. **`bin/{test,check,build}`** still pass `--exclude jax-desktop` flags that will now error since the crate is gone. Same call as above — outside your scope, but flagging because they'll break the next CI run if left.

Carry on. Wiki phase 1 looked great in the broadcast.
