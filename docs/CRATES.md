# Zim Crate Layout

Target shape of the Zim workspace. Clean break — no compat shims, no phased migration, no deprecation. Owner of this doc: thing1 (T-005). Binding policy: `.coord/broadcast/20260524T014147Z-clean-break-policy.md`.

## Target workspace

```
crates/
├── zim-crypto/     # Ed25519/X25519, ChaCha20-Poly1305, secret sharing
├── zim-fs/         # Filesystem: manifest, paths, CRDT path ops, nodes, conflicts
├── zim-store/      # Content store: blob storage + content addressing
├── zim-protocol/   # Wire protocol: peer messaging, sync, handshake, append log
├── zim-peer/       # System daemon binary (zim) + HTTP API + FUSE + DB
└── zim-hub/        # Read-only web mirror gateway, Google-auth-guarded key
```

That is the whole workspace. Nothing else lives in `crates/`.

## Current → target mapping

| Source | Destination |
|---|---|
| `crates/common/src/crypto/` | `crates/zim-crypto/src/` |
| `crates/common/src/mount/` | `crates/zim-fs/src/fs/` *(module renamed `mount` → `fs`; types: `MountInner` → `FsInner`, etc.)* |
| `crates/common/src/linked_data/` | `crates/zim-fs/src/linked_data/` |
| `crates/common/src/peer/` | `crates/zim-protocol/src/peer/` |
| `crates/common/src/bucket_log/` | `crates/zim-protocol/src/log/` |
| `crates/common/src/version.rs` | `crates/zim-peer/src/version.rs` *(build-time only)* |
| `crates/object-store/` | `crates/zim-store/` *(package rename, no re-exports back)* |
| `crates/daemon/` | `crates/zim-peer/` *(binary `jax` → `zim`)* |
| `crates/common/` | **deleted** *(emptied by the moves above)* |
| `crates/desktop/` | **deleted** |
| `crates/app/` | **deleted** |
| *(new)* | `crates/zim-hub/` |

## Dependency graph

Strict DAG. Arrows mean "depends on". No cycles, no upward edges.

```
zim-hub  ─┬─→ zim-protocol ─┬─→ zim-fs ─┬─→ zim-store ─→ (iroh-blobs)
          │                 │           │
          │                 │           └─→ zim-crypto
          │                 │
          │                 └─→ zim-crypto
          │
          ├─→ zim-fs
          ├─→ zim-store
          └─→ zim-crypto

zim-peer ─┬─→ zim-protocol
          ├─→ zim-fs
          ├─→ zim-store
          └─→ zim-crypto
```

Rules:

- **`zim-crypto`** is a leaf — no workspace deps.
- **`zim-store`** is a leaf — pure blob storage. **No crypto inside.** Encryption is done by callers (`zim-fs`, `zim-protocol`) before bytes hit the store.
- **`zim-fs`** depends on `zim-store` + `zim-crypto`.
- **`zim-protocol`** depends on `zim-fs` + `zim-crypto`.
- **`zim-peer`** and **`zim-hub`** are the two binary crates. They sit on top of everything. **They do not depend on each other.**

## Naming and package strategy

| Aspect | Convention |
|---|---|
| Directory | `crates/zim-<name>/` (kebab-case) |
| Cargo package `name` | `zim-<name>` (kebab-case, no `jax-` prefix anywhere) |
| Library `name` (Rust import path) | `zim_<name>` (snake_case) |
| Binary | `zim` (the only binary; was `jax`) |
| Versions | Reset to `0.1.0` on rename. Pre-1.0, no semver continuity claim. |
| Workspace metadata | `repository`, `homepage`, keywords updated to the zim repo. |
| Module names | No `mount`. The filesystem module is `fs`. Types: `Fs`, `FsInner`, `FsNode`, etc. |
| No-no list | No `core`, no `mount`, no `jax`, no `// DEPRECATED`, no compat shims. |

## Cut-over sequence

Single cut-over. Not a phased migration. All of the below lands together (one commit, or one tight series of commits with no green-checkpoint requirement between them — the only required green state is the end state).

1. **Create the six new crate directories** with `Cargo.toml` + `src/lib.rs` (or `src/main.rs` for binaries). Add all six to workspace `members`. Remove `crates/common`, `crates/daemon`, `crates/object-store`, `crates/desktop/src-tauri` from `members`.

2. **Move files** per the mapping table above. `git mv` for history; rename modules in lockstep with the move.

3. **Rename `mount` → `fs`** inside `zim-fs`: directory, module declarations, type names (`MountInner` → `FsInner`, `MountConfig` → `FsConfig`, etc.), function signatures, doc text. No backwards-compatible aliases.

4. **Rename binary `jax` → `zim`** in `zim-peer/Cargo.toml`, `src/main.rs`, all CLI strings, `bin/dev`, install scripts.

5. **Update every `use` path** workspace-wide: `common::crypto::*` → `zim_crypto::*`, `common::mount::*` → `zim_fs::fs::*`, `common::peer::*` → `zim_protocol::peer::*`, etc. No re-exports from a defunct `common`.

6. **Delete** `crates/common/`, `crates/desktop/`, `crates/app/` entirely.

7. **Create `crates/zim-hub/`** as a minimal axum skeleton with the dep wiring shown in the graph above. No real handlers yet — just the buildable shape.

8. **Update root metadata** (`Cargo.toml` workspace package fields), `README.md`, `docs/PROJECT_LAYOUT.md`, `docs/index.md` to describe the new shape only — no "previously known as" footnotes.

End state must satisfy: `cargo build && cargo test && cargo clippy -- -D warnings && cargo fmt -- --check`. That is the only checkpoint.

## What this doc deliberately does not cover

- Internal API redesign inside any crate. Code moves, then we redesign in follow-ups.
- HTTP API surface of `zim-hub` — a separate task.
- Database schema — untouched.
- Feature work — out of scope until the layout is in place.
