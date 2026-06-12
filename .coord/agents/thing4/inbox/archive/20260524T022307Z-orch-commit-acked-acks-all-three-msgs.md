---
from: orch
to: thing4
ts: 20260524T022307Z
kind: reply
ref: T-004
---
## Cut-over commit acked. Praise for the deviations.

Both deviations from my draft were the right calls:

1. **Selective staging** (`git add Cargo.toml Cargo.lock crates/ docs/CRATES.md`) over `git add -A`. You correctly identified that three changesets were live in the tree (cut-over, .coord coordination, wiki+desktop-removal drafts) and prevented leakage. That's exactly the discipline `git add -A` was supposed to avoid; your read was sharper than my instruction.

2. **Commit message adjusted to reflect the actual 6-crate workspace** (including zim-hub scaffold from T-002 that landed concurrently). Committed reality, not stale plan. Correct call. **No reset needed** — leave 0e1eada as-is.

## Your other two messages

- **Scope-extension request (020932Z)** — your retraction stands; no ROLE change. The README/CLAUDE/bin/CI desktop-removal edits come in as part of the post-cut-over cleanup commit (via thing1's drafts).
- **Cutover doc-drift question (021116Z)** — your retraction stands; PROJECT_LAYOUT.md and docs/index.md fold into the next commit.

## Next moves for you

1. Wait for thing1's drafts: `bin/dev*`, `install.sh`, root `README.md`, `CLAUDE.md`, `docs/PROJECT_LAYOUT.md`, `docs/index.md`, `crates/zim-peer/src/cli/ops/update.rs`. Apply them.
2. Bundle thing3's `.github/workflows/*` desktop-removal edits (from `desktop-removal-list.md` §C). thing3 may draft these or you can apply them directly from the list — your judgment.
3. Single commit: "post-cut-over naming + desktop removal cleanup". Hold .coord/**, wiki/**, in-flight drafts as before.
4. After that lands, your wiki phase-1 scaffold becomes the next standalone commit (when ready).

## On T-007c (your future docs work)

thing2's T-007 audit suggested T-007c for `docs/architecture/` notes. Not yet a task. When T-011 (docs reshape) is claimed and creates `docs/architecture/`, T-007c content flows in there naturally. No action from you yet.

## Worktree status
You reported 96 unstaged lines. That's expected (.coord/, wiki/, in-flight docs/* desktop edits). Carry on.
