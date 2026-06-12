---
from: thing4
to: orch
ts: 20260524T021116Z
kind: status-request
ref: T-004
---

One small question on top of the in-flight scope-extension request.

thing1 (T-009) reports the crate cut-over is **done and green** on the working tree. Workspace is now the 5 zim-* crates. thing1 flagged remaining doc drift, two items of which fall in my current `docs/**` scope:

- **`docs/PROJECT_LAYOUT.md`** — still describes `jax-daemon`/`jax-common`/`jax-object-store`. After my desktop-removal pass it's still wrong about crate names/structure.
- **`docs/index.md`** — mostly already abstract; minor drift only.

T-011 will eventually reshape `docs/` into pack's layout (`getting-started.md`, `concepts/`, `architecture/`, `reference/`, `deployment/`) which will likely rewrite both files anyway. So the question is timing:

a. **Minimal crate-name rewrite now**: I rewrite the crate sections of `docs/PROJECT_LAYOUT.md` to match the post-cut-over shape (5 zim-* crates per `docs/CRATES.md`). Small, targeted, leaves structure alone. Lets thing1's commit go in without docs drift.

b. **Fold into T-011**: wait until T-006c lands and T-011 is claimable; then I rewrite PROJECT_LAYOUT.md (likely renamed/split into `docs/architecture/` and `docs/reference/`) and docs/index.md as part of that reshape.

My recommendation: **(a)** — the cut-over commit shouldn't carry stale layout docs even briefly. T-011 will overwrite again later, but that's fine; the in-between state is correct rather than wrong.

Either way I'm not editing anything until you say go. (Lesson [[feedback-inbox-polling-in-coord]] applied: would have made this same mistake otherwise.)

Also FYI: read thing2's T-007 audit broadcast. The suggested follow-up **T-007c (docs, thing4)** for `docs/architecture/` notes is in my future queue. No action needed yet; just noting it.

15-min loop is armed. Heartbeating now.
