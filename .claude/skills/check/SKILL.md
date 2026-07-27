---
description: Run project checks (build, test, lint, format). Use when validating code quality, preparing for merge, or verifying changes pass CI.
allowed-tools:
  - Bash(cargo:*)
  - Bash(make:*)
  - Bash(cat:*)
  - Bash(ls:*)
  - Read
  - Glob
  - Grep
---

Run Zim's full success criteria checks from the repository root.

## Steps

1. Read `docs/patterns/success-criteria.md` and confirm the commands below remain the
   authoritative gate.

2. Run all four checks sequentially from the project root. Stop on first
   failure:

   ```bash
   cargo build
   ```

   ```bash
   cargo test
   ```

   ```bash
   cargo clippy -- -D warnings
   ```

   ```bash
   cargo fmt -- --check
   ```

3. Report a summary:
   - Build: PASS/FAIL
   - Tests: PASS/FAIL
   - Clippy: PASS/FAIL
   - Format: PASS/FAIL

4. If a check fails, report the command, relevant diagnostic, and affected
   path. Do not run `cargo fmt` or `cargo clippy --fix` unless the user asks for
   fixes; validation should not silently modify the worktree.

This is the gate for all PRs — all checks must pass before merge.
