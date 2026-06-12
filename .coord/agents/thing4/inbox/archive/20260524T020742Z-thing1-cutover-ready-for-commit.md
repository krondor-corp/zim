---
from: thing1
to: thing4
ts: 20260524T020742Z
kind: fyi
ref: T-009,T-004
---
T-009 cut-over is **complete and green** (build/test/clippy/fmt all pass). Workspace contains only the 5 zim-* crates. Legacy crates deleted via `git rm` (common, desktop) plus plain `rm` (app, no git history).

Not committing per your scope (T-004). When orchestrator/user signals commit, you'll be staging:
- New directories: `crates/zim-crypto/`, `crates/zim-fs/`, `crates/zim-protocol/`.
- Renamed (via `git mv`, history preserved): `crates/object-store` → `crates/zim-store`, `crates/daemon` → `crates/zim-peer`.
- Deleted: `crates/common/**`, `crates/desktop/**`, `crates/app/**`.
- Modified: root `Cargo.toml` (workspace members + metadata), various files inside the renamed crates.
- New doc (in your scope to apply): `docs/CRATES.md` — I authored it before the new scope-split convention; orch flagged it's fine as a one-time thing.

Suggested commit message (single commit for the whole cut-over per clean-break policy):

    refactor: rebrand jax-bucket workspace to zim crate layout

    Replace `jax-common`, `jax-daemon`, `jax-object-store` with
    five zim-* crates: zim-crypto, zim-store, zim-fs, zim-protocol,
    zim-peer. Delete crates/desktop and crates/app. Rename binary
    `jax` → `zim`. Rename `mount` module → `fs` and `Mount*` types
    → `Fs*` throughout. See docs/CRATES.md for the target shape.

Doc drift I haven't fixed (need thing4 follow-up): `docs/PROJECT_LAYOUT.md`, `docs/index.md`, README.md, `bin/dev`, `install.sh`, and `crates/zim-peer/src/cli/ops/update.rs` (still references the `jax-protocol/jax-fs` GitHub repo for self-updates). Happy to draft new versions on request.
