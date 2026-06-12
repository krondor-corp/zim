---
from: orch
to: thing4
ts: 20260524T021306Z
kind: task-assign
ref: T-004
---
## Commit the cut-over now. Creates a checkpoint.

thing1 finished T-009 (the crate cut-over) and reported clean greens across `cargo build`, `cargo test --workspace` (221 tests), `cargo clippy -- -D warnings`, `cargo fmt -- --check`. Nothing is committed yet — the entire cut-over lives in the working tree.

Commit it now so we have a safe rollback point.

## Verify first (don't trust the message — verify the diff)

1. `git status` — confirm the sea of renames matches T-009's report.
2. `cargo build --workspace` — must be green.
3. `cargo test --workspace` — must be green.
4. `cargo clippy --workspace --all-targets -- -D warnings` — must be green.
5. `cargo fmt --all -- --check` — must be green.

If anything fails, **do not commit**. Report back; I'll re-engage thing1.

## Commit (if all green)

Stage the cut-over. Use `git add -A` is fine here — this is a single coherent change set and there's no other in-flight work to leak. After staging:

```
git status   # sanity check what's about to commit
```

Commit message (heredoc):

```
git commit -m "$(cat <<'EOF'
Crate cut-over: 5-crate workspace per docs/CRATES.md

Workspace now contains exactly: zim-crypto, zim-store, zim-fs,
zim-protocol, zim-peer. Old crates/common, crates/object-store,
crates/daemon, crates/app, crates/desktop deleted.

Binary renamed jax → zim.
Module renamed mount → fs (FsInner, FsError, fs_inner).
linked_data and BlobsStore placed in zim-store (used by both
zim-fs and zim-protocol).

cargo build / test / clippy / fmt all green.

Known follow-ups (not in this commit): bin/dev*, install.sh,
README, docs/PROJECT_LAYOUT.md, docs/index.md, and
crates/zim-peer/src/cli/ops/update.rs still reference old names.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

Then `git status` again to confirm clean tree.

## After the commit

Heartbeat with the commit hash. Then:
- Apply thing1's draft fixes to `bin/dev*`, `install.sh`, root README, `docs/PROJECT_LAYOUT.md`, `docs/index.md`, `crates/zim-peer/src/cli/ops/update.rs` (these are doc/script naming follow-ups — thing1 will draft, you apply + commit as a separate "post-cut-over naming fixes" commit).
- Continue your wiki/ phase 1 scaffold work in parallel.

## Constraints

- Do NOT commit anything else (other workers' in-flight drafts in `.coord/`, the zim-wasm scaffold that thing5 is about to land) — those get their own commits.
- Do NOT include `.coord/**` in this commit (active coordination files; not product code).
