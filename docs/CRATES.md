# Zim Crate Layout

The shipped shape of the Zim workspace. 7 crates: 2 binaries, 5 libraries.

## Workspace

```
crates/
├── zim-crypto/    # Ed25519/X25519 identity, ChaCha20-Poly1305, X25519 secret sharing
├── zim-core/      # Core: filesystem (manifest, nodes, CRDT path ops, conflict resolution, publication), content store, linked data, iroh abstraction
├── zim-protocol/  # Wire protocol: peer messaging, sync jobs, handshake, append-only bucket log
├── zim-runtime/   # Service trait + ShutdownHandle (shared lifecycle plumbing) — leaf crate
├── zim-peer/      # System daemon binary `zim` + HTTP API + FUSE + SQLite DB (also a library)
├── zim-hub/       # Web gateway binary `zim-hub` — embeds an in-process peer, Askama + Datastar UI, Google OAuth identity vault
└── zim-wasm/      # Browser-side WASM client (client-side decryption of published blobs)
```

`zim-core` consolidates what was previously `zim-fs` + `zim-store` — the filesystem primitives and the content-addressed blob storage live together because they share types (`Link`, `Hash`, `Leaf`, `Secret`) and the separation created more cross-crate plumbing than it saved.

## Dependency graph

Strict DAG. Arrows mean "depends on" (Cargo `path` dep).

```
zim-wasm  ─→ zim-crypto (wasm feature, no iroh)

zim-hub   ─┬─→ zim-peer (default-features = false)
           ├─→ zim-protocol
           ├─→ zim-core
           └─→ zim-runtime

zim-peer  ─┬─→ zim-protocol
           ├─→ zim-core
           ├─→ zim-crypto
           └─→ zim-runtime

zim-protocol ─┬─→ zim-core
              └─→ zim-crypto

zim-core ─→ zim-crypto

zim-crypto: leaf (no workspace deps; wraps iroh keys under default `iroh-keys` feature)
zim-runtime: leaf (no workspace deps; tokio + futures + async-trait)
```

Rules:

- **`zim-crypto`** wraps `iroh::PublicKey`/`SecretKey` by default (`iroh-keys` feature). The `wasm` feature falls back to `ed25519-dalek` directly so the crate compiles to `wasm32-unknown-unknown` for `zim-wasm`.
- **`zim-core`** owns the filesystem types (`Manifest`, `Node`, `Leaf`, `Share`, `Relay`, `Published`, `Pins`), the content store (`BlobsStore`, `ObjectStore`), and the content-addressing primitives (`Link`, `Hash`, `Cid`). Both `zim-protocol` (which ships manifests over the wire) and `zim-peer` (which provides the HTTP/CLI surface) depend on it.
- **`zim-protocol`** depends on `zim-core` + `zim-crypto`. Houses the iroh peer, sync jobs, handshake, and the append-only bucket log.
- **`zim-runtime`** is a leaf — `Service` trait + `ShutdownHandle`. Both binaries embed it.
- **`zim-peer`** is both a library and the headless daemon binary. It exposes a library API so `zim-hub` can embed peer services in-process.
- **`zim-hub`** depends on `zim-peer` (with `default-features = false` to exclude FUSE). The hub embeds a peer as a Relay — see the "zim-hub embeds peer" broadcast.

## Naming conventions

| Aspect | Convention |
|---|---|
| Crate directory | `crates/zim-<name>/` (kebab-case) |
| Cargo package `name` | `zim-<name>` (kebab-case) |
| Library `name` (Rust import path) | `zim_<name>` (snake_case) |
| Binaries | `zim` (in `zim-peer`), `zim-hub` (in `zim-hub`) |
| Versions | All pre-1.0. No semver continuity to the previous `jax-*` crates. |
| Module names | No `mount` — the filesystem module is `fs`. Types: `Fs`, `FsInner`, `FsNode`. The one literal "mount" in the tree is `crates/zim-peer/src/fuse/mount_manager.rs` (the POSIX concept). |
| No-no list | No `core` as a module name (the crate is `zim-core` but inside it the top-level module is `fs`), no `mount`, no `jax`, no `// DEPRECATED`, no compat shims. |

## Module layout conventions

- **Handler-per-file** under `http/views/` (server-rendered HTML) and `http/api/` (JSON), used by `zim-hub` and `zim-peer`.
- **`runtime::Service` + `ShutdownHandle`** in `zim-runtime`. Both binaries push named service handles and `.wait()` for SIGINT/SIGTERM.
- **Templates**: Askama for server-rendered HTML in `zim-hub`. Hypermedia client: Datastar (not HTMX).
- **CLI Op pattern** in `zim-peer/src/cli/`: commands return typed data, format via `Display` in the binary.

## What this doc does not cover

- Internal API design inside any crate — see `docs/architecture/project-layout.md`.
- HTTP surface details — `zim-peer`'s API is in `docs/reference/api.md`; `zim-hub`'s views are in `crates/zim-hub/README.md`.
- Database schema — lives next to the owning crate (migrations in `zim-peer/migrations/` and `zim-hub/migrations/`).
- Feature gates — `zim-peer`'s `fuse` feature, `zim-crypto`'s `iroh-keys`/`wasm` features — documented at the crate level.
