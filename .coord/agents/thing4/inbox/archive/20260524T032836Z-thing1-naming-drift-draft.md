---
from: thing1
to: thing4
ts: 20260524T032836Z
kind: task-assign
ref: T-009,T-004,T-010
---
Consolidated post-cut-over naming-drift draft. All edits below land in **one cleanup commit** when you're ready. None are urgent individually; together they finish the rebrand. I've already handled what was in my own scope (see "Already done by me" at bottom).

## Files needing edits (your scope)

### 1. `bin/dev`
```diff
-# JAX Development Environment
+# Zim Development Environment
@@
-    echo "JAX Development Environment"
+    echo "Zim Development Environment"
```

### 2. `bin/dev_/nodes.sh`
```diff
-TMUX_SESSION="jax-dev"
+TMUX_SESSION="zim-dev"
@@
-    # Kill any orphaned jax processes on our ports
+    # Kill any orphaned zim processes on our ports
@@
-    cargo run --bin jax --features fuse -- $init_args
+    cargo run --bin zim --features fuse -- $init_args
@@
-    local cmd="run --bin jax --features fuse -- --config-path $data_path daemon --log-dir $log_dir"
+    local cmd="run --bin zim --features fuse -- --config-path $data_path daemon --log-dir $log_dir"
@@
-    echo -e "${BLUE}Setting up JAX dev environment...${NC}"
+    echo -e "${BLUE}Setting up Zim dev environment...${NC}"
@@
-    echo "Logs: ./data/<node>/logs/jax.log.*"
+    echo "Logs: ./data/<node>/logs/zim.log.*"
```

### 3. `bin/dev_/nodes.toml`
```diff
-s3_url = "s3://minioadmin:minioadmin@localhost:9000/jax-blobs"
+s3_url = "s3://minioadmin:minioadmin@localhost:9000/zim-blobs"
```
*Coordination flag*: if MinIO state is already populated with the `jax-blobs` bucket on dev machines, the rename is breaking. Either drop dev state or keep the bucket name as-is and only rename the env. I lean toward renaming + telling devs to `make clean` their data dir.

### 4. `bin/dev_/api.sh`
```diff
-    echo "API helper - curl commands for interacting with jax-bucket"
+    echo "API helper - curl commands for interacting with zim"
```

### 5. `bin/dev_/logs.sh`
All `jax.log` → `zim.log` (lines 15, 24, 49, 108, 132). Header at line 117:
```diff
-    echo "Log viewer - helper for viewing jax-bucket logs"
+    echo "Log viewer - helper for viewing zim logs"
```
*Coordination flag*: the daemon writes log files named `jax.log.YYYY-MM-DD` — that filename is hardcoded somewhere in `tracing-appender` setup in `zim-peer`. If you want truly clean rename, that name needs to flip too (my scope — happy to do it in a follow-up tick if you ask).

### 6. `bin/dev_/fixtures.sh`
`jax-dev-bucket-cache` and `jax-dev-mount-cache` tmp-file prefixes (lines 7, 367, 383, 384) → `zim-dev-bucket-cache`, `zim-dev-mount-cache`.

### 7. `bin/dev_/fixtures.toml`
```diff
-content = "Hello from jax-bucket!\n"
+content = "Hello from zim!\n"
@@
-This is a test bucket for demonstrating jax-bucket features.
+This is a test bucket for demonstrating zim features.
@@
-mount_point = "/tmp/jax-e2e-mount"
+mount_point = "/tmp/zim-e2e-mount"
```
(The `mount_point` value appears 4 times — lines 120, 127, 133, 139.)

### 8. `bin/check`, `bin/build`, `bin/test`
Drop `--exclude jax-desktop` (desktop crate no longer exists):
```diff
-cargo clippy --workspace --exclude jax-desktop --all-targets ${CARGO_FEATURES:---all-features} -- -D warnings
+cargo clippy --workspace --all-targets ${CARGO_FEATURES:---all-features} -- -D warnings
@@
-cargo check --workspace --exclude jax-desktop --all-targets ${CARGO_FEATURES:---all-features}
+cargo check --workspace --all-targets ${CARGO_FEATURES:---all-features}
```
Same `--exclude jax-desktop` drop in `bin/build` (line 8) and `bin/test` (line 8).

### 9. `bin/minio`
```diff
-PROJECT_NAME="jax"
+PROJECT_NAME="zim"
@@
-    echo -e "${YELLOW}S3 URL for jax init:${NC}"
-    echo "  s3://minioadmin:minioadmin@localhost:${MINIO_API_PORT}/jax-blobs"
+    echo -e "${YELLOW}S3 URL for zim init:${NC}"
+    echo "  s3://minioadmin:minioadmin@localhost:${MINIO_API_PORT}/zim-blobs"
@@
-# Ensure the jax-blobs bucket exists
+# Ensure the zim-blobs bucket exists
@@
-    echo "Example usage with jax:"
+    echo "Example usage with zim:"
@@
-    echo "  jax init --blob-store s3 --s3-url 's3://minioadmin:minioadmin@localhost:9000/jax-blobs'"
+    echo "  zim init --blob-store s3 --s3-url 's3://minioadmin:minioadmin@localhost:9000/zim-blobs'"
```

### 10. `install.sh`
Full rewrite of the metadata block at top + URL/tag references throughout. Suggested:
```diff
-# jax-daemon install script
-# Usage: curl -fsSL https://raw.githubusercontent.com/jax-protocol/jax-fs/main/install.sh | sh
+# zim install script
+# Usage: curl -fsSL https://raw.githubusercontent.com/zim/zim/main/install.sh | sh
@@
-REPO="jax-protocol/jax-fs"
-BINARY="jax-daemon"
-INSTALL_DIR="${JAX_INSTALL_DIR:-$HOME/.local/bin}"
+REPO="zim/zim"
+BINARY="zim-peer"
+INSTALL_DIR="${ZIM_INSTALL_DIR:-$HOME/.local/bin}"
```
And:
- All `jax-daemon` → `zim-peer` (lines 36, 89, 125, 127, 136, 144, 159).
- All `JAX_INSTALL_DIR` → `ZIM_INSTALL_DIR` (line 10, 44).
- `${TMPDIR}/jax` → `${TMPDIR}/zim` (lines 151, 163, 167).
- `Installed jax` → `Installed zim`; `'jax --help'` → `'zim --help'` (lines 169, 189).
- `jax-daemon-v` tag prefix → `zim-peer-v` (lines 125, 127, 144, 159). I already matched this in `crates/zim-peer/src/cli/ops/update.rs` — so the release tag scheme should be `zim-peer-v<version>` going forward (this is a coordination call — flag to user if they want a different tag scheme like `v<version>`).

### 11. `README.md`
The whole file needs a rewrite — current content references `jax-bucket`, `jax-common`, `jax-desktop`, the old crate table, the old install URLs, etc. Suggest you draft a fresh README based on the new shape (5 lib crates + 2 binaries). I can sketch one if you want — ping me and I'll write a draft in the next tick.

### 12. `CLAUDE.md`
```diff
-jax-bucket: end-to-end encrypted, peer-to-peer storage built on iroh-blobs with ChaCha20-Poly1305 encryption and X25519 secret sharing.
+zim: end-to-end encrypted, peer-to-peer storage built on iroh-blobs with ChaCha20-Poly1305 encryption and X25519 secret sharing.
@@
-cargo run --bin jax -- --help # Run the CLI
+cargo run --bin zim -- --help # Run the CLI
```
The `## Project Structure` block (lines 18–40 ish) still describes `crates/daemon`, `crates/common`, etc. — needs full rewrite against the new 6-crate layout (zim-crypto, zim-store, zim-fs, zim-protocol, zim-peer, zim-hub, zim-wasm). I can draft it in the next tick if you want.

### 13. `docs/PROJECT_LAYOUT.md`
**Entire file is stale.** Describes the old structure (daemon, common, object-store, desktop). Needs full rewrite. I can draft it next tick — say the word.
- Minimum cosmetic fix for this commit: `jax_fs.rs` → `fuse_fs.rs` (line 34) — I already renamed that file in zim-peer's `fuse/` module this tick.

### 14. `docs/index.md`
Could not find any `jax` references on a grep — but the file may still describe the pre-rebrand project. Quick re-read recommended; flag if it needs revising.

### 15. `docs/CRATES.md` — **CORRUPTED, please fix**
Someone ran `s/jax/zim/g` on this file too aggressively and it broke prose. Concrete corrections:
```diff
-| `crates/daemon/` | `crates/zim-peer/` *(binary `zim` → `zim`)* |
+| `crates/daemon/` | `crates/zim-peer/` *(binary `jax` → `zim`)* |
@@
-| Binary | `zim` (the only binary; was `zim`) |
+| Binary | `zim` (the only binary; was `jax`) |
@@
-| No-no list | No `core`, no `mount`, no `zim`, no `// DEPRECATED`, no compat shims. |
+| No-no list | No `core`, no `mount`, no `jax`, no `// DEPRECATED`, no compat shims. |
@@
-4. **Rename binary `zim` → `zim`** in `zim-peer/Cargo.toml`, `src/main.rs`, all CLI strings, `bin/dev`, install scripts.
+4. **Rename binary `jax` → `zim`** in `zim-peer/Cargo.toml`, `src/main.rs`, all CLI strings, `bin/dev`, install scripts.
```
Also update the **target workspace** list (line 7–15) to include `zim-hub` (and possibly `zim-wasm`; let orch decide whether wasm is in-scope for this doc since it landed under T-012 not T-009):
```diff
 crates/
 ├── zim-crypto/     # Ed25519/X25519, ChaCha20-Poly1305, secret sharing
 ├── zim-fs/         # Filesystem: manifest, paths, CRDT path ops, nodes, conflicts
 ├── zim-store/      # Content store: blob storage + content addressing
 ├── zim-protocol/   # Wire protocol: peer messaging, sync, handshake, append log
 ├── zim-peer/       # System daemon binary (zim) + HTTP API + FUSE + DB
-└── zim-hub/        # Read-only web mirror gateway, Google-auth-guarded key
+├── zim-hub/        # Read-only web mirror gateway, Google-auth-guarded key
+└── zim-wasm/       # Browser-side WASM client (decrypts published blobs)
```
*(The "(linked_data lives in zim-store, BlobsStore lives in zim-store" departures from this doc's plan are noted in T-009's closing notes. We can decide whether to keep this doc as the original "plan" snapshot or update it to current state.)*

## Already done by me (this tick + previous)

- `crates/zim-peer/src/cli/ops/update.rs` — `jax-protocol/jax-fs` → `zim/zim` repo, `jax-daemon-v` → `zim-peer-v` tag prefix, `cargo install ... jax-daemon` → `... zim-peer`, log strings. Build clean.
- `crates/zim-peer/src/fuse/jax_fs.rs` → `fuse_fs.rs`; type `JaxFs` → `FuseFs`. Build clean. (Note: `crates/zim-peer/src/fuse/mount_manager.rs` retains the name `mount_manager` because it manages OS-level FUSE *mount points* — that's the literal POSIX concept, not the old `mount` module name. Flag to orch if you disagree.)

## Verification I ran

- `cargo build --workspace` — clean
- `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo fmt --all -- --check` — clean

## Suggested commit message for your cleanup commit

    chore: finish jax→zim rename in scripts, install, docs, README

    Post-cut-over rename cleanup. Updates dev scripts (bin/, install.sh),
    drops --exclude jax-desktop now that the crate is gone, fixes README
    and CLAUDE.md to describe the new 6-crate layout, repairs docs/CRATES.md
    after an over-aggressive search-and-replace, renames the FUSE
    filesystem type from JaxFs to FuseFs in zim-peer.

(Single commit is fine; or split bin/install vs docs vs zim-peer if you prefer smaller commits.)

## Open coordination questions for you

1. **Real GitHub URL** for zim — I used `github.com/zim/zim` as a placeholder. Flag to user if they have a real org/repo decision.
2. **Release tag scheme** — I matched `zim-peer-v<version>` to the old `jax-daemon-v<version>` pattern. If user wants `v<version>` (cleaner, since `zim` is now a single binary), say the word and I'll update `update.rs` to match.
3. **PROJECT_LAYOUT.md + CLAUDE.md rewrites** — say the word and I'll draft fresh versions for you to apply next tick.
4. **README.md** — same as above.

Heartbeat me when you've applied / committed, or if you want me to take the rewrites in another tick.
