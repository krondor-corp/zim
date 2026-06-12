---
from: thing1
to: thing4
ts: 20260524T170157Z
kind: task-assign
ref: T-010
---
T-010 deliverable: revised `docs/CRATES.md`. Current doc is stale on every section — written before the cut-over actually shipped, missing `zim-runtime`/`zim-wasm`, dependency graph predates `zim-hub → zim-peer` (T-015), cut-over sequence section is now historical artifact.

**Apply the full file content below as a replacement of `docs/CRATES.md`.** Single commit fine. No content from my side touches `docs/` directly.

---

```markdown
# Zim Crate Layout

The shipped shape of the Zim workspace. Aesthetic reference: [`krondor-corp/pack`](https://github.com/krondor-corp/pack) (pack is aesthetic-only — see `.coord/broadcast/20260524T015636Z-pack-design-language.md`); structural divergence from pack's 3-crate shape is intentional and documented below.

## Workspace

```
crates/
├── zim-crypto/     # Ed25519/X25519 identity, ChaCha20-Poly1305 content encryption, X25519 secret sharing
├── zim-store/      # Content-addressed blob store: SQLite + S3/MinIO/local + iroh-blobs wrapper + linked_data (Hash/Cid/Link)
├── zim-fs/         # Filesystem: manifest, nodes, CRDT path ops, conflict resolution, published-set entries
├── zim-protocol/   # Wire protocol: peer messaging, sync jobs, handshake, append-only bucket log
├── zim-runtime/    # Service trait + ShutdownHandle (shared lifecycle plumbing) — leaf crate
├── zim-peer/       # System daemon binary `zim` + HTTP API + FUSE + SQLite DB (also a library)
├── zim-hub/        # Web gateway binary `zim-hub` — embeds an in-process peer (Mirror), Askama + Datastar UI
└── zim-wasm/       # Browser-side WASM client for the hub (client-side decryption of published blobs)
```

Two binaries, six libraries. `zim-wasm` is its own thing — a `cdylib + rlib` that the hub vendors as static assets, not consumed via Cargo by any other crate.

## Dependency graph

Strict DAG. Arrows mean "depends on" (Cargo path-dep).

```
zim-wasm  ─→ zim-crypto (wasm feature, no iroh)

zim-hub   ─┬─→ zim-peer ──┐
           ├─→ zim-protocol ─┐
           ├─→ zim-fs       ─┤
           ├─→ zim-store    ─┤
           └─→ zim-runtime  ─┤
                             │
zim-peer  ─┬─→ zim-protocol ─┤
           ├─→ zim-fs       ─┤
           ├─→ zim-store    ─┤
           ├─→ zim-crypto   ─┤
           └─→ zim-runtime  ─┘

zim-protocol ─┬─→ zim-fs
              ├─→ zim-store
              └─→ zim-crypto

zim-fs    ─┬─→ zim-store
           └─→ zim-crypto

zim-store ─→ zim-crypto

zim-crypto: leaf (no workspace deps; depends on `iroh` under the default `iroh-keys` feature)
zim-runtime: leaf (no workspace deps)
```

Rules:

- **`zim-crypto`** wraps `iroh::PublicKey`/`SecretKey` by default (`iroh-keys` feature). The `wasm` feature falls back to `ed25519-dalek` directly so the crate compiles to `wasm32-unknown-unknown` for `zim-wasm`.
- **`zim-store`** owns content-addressing primitives (`linked_data::{Hash, Cid, Link, BlockEncoded, DagCborCodec}`) and the iroh-blobs serving wrapper (`BlobsStore`). Both `zim-fs` and `zim-protocol` consume these — that's why they live here rather than in `zim-fs`.
- **`zim-fs`** depends on `zim-store` (Link, BlobsStore) and `zim-crypto` (manifest principals, envelope keys). It is the home of the filesystem types and the CRDT path-op log.
- **`zim-protocol`** depends on `zim-fs` (ships manifests over the wire), `zim-store` (Hash/Link), `zim-crypto` (identity, handshake).
- **`zim-runtime`** is a leaf — `Service` trait + `ShutdownHandle`. Both binaries embed it; aesthetic adopted from pack.
- **`zim-peer`** is both a library and the headless daemon binary. It exposes `spawn_peer_services(config, &mut handle, shutdown_rx) -> ServiceState` as the shared spawning surface; `zim-hub` calls into the same library code rather than running a separate process.
- **`zim-hub`** depends on `zim-peer` — the hub embeds a peer in-process as a Mirror (see `.coord/broadcast/20260524T040247Z-zim-hub-embeds-peer.md`). Earlier drafts of this doc said the two binaries don't depend on each other; the embed-peer pivot changed that.

## Naming and package conventions

| Aspect | Convention |
|---|---|
| Crate directory | `crates/zim-<name>/` (kebab-case) |
| Cargo package `name` | `zim-<name>` (kebab-case) |
| Library `name` (Rust import path) | `zim_<name>` (snake_case) |
| Binaries | `zim` (in `zim-peer`), `zim-hub` (in `zim-hub`) |
| Versions | All pre-1.0; reset to `0.1.0` on the cut-over commit. No semver continuity to the previous `jax-*` crates. |
| Module names | No `mount` — the filesystem module is `fs`, types are `Fs`/`FsInner`/`FsNode`. The one literal "mount" in the tree is `crates/zim-peer/src/fuse/mount_manager.rs` — that manages OS-level FUSE mount points (the POSIX concept), not the old filesystem-module name. |
| No-no list | No `core`, no `mount` (module names), no `jax`, no `// DEPRECATED`, no compat shims. |

## Module layout aesthetics (pack-aligned, where it fits)

These are adopted from pack as conventions — they apply per-crate, not as crate-shape rules.

- **Handler-per-file** under `http/views/` (server-rendered HTML) and `http/api/` (JSON RPC), already used by `zim-hub` and partially by `zim-peer`. Each verb has its own file.
- **`runtime::Service` + `ShutdownHandle`** lives in `zim-runtime`. Both binaries push named handles (`"peer"`, `"sync"`, `"http"`, `"api"`, `"gateway"`, `"fuse-drain"`) onto a single `ShutdownHandle` and `.wait()` for SIGINT/SIGTERM.
- **Templates**: Askama for server-rendered HTML in `zim-hub`. Hypermedia client: **Datastar**, vendored as a single JS file under `crates/zim-hub/static/vendor/`. Pack uses HTMX — Zim explicitly diverges (see binding broadcast).
- **CLI Op pattern** in `zim-peer/src/cli/`: each command returns typed data, formats via `Display` in the binary. Documented in `docs/CLI.md`.

## Divergence from pack's `core` / `crdt` / `app` shape

Pack splits its workspace into three crates: `core` (data + business logic), `crdt` (Yjs-backed collaboration), `app` (server + tasks + MCP). Zim does not collapse to that shape. Reasons:

| Pack | Zim equivalent | Why we keep it separate |
|---|---|---|
| `core` | Would be a fusion of `zim-fs` + `zim-store` + `zim-crypto` | `zim-store` is the only crate that has to compile both natively (iroh-blobs + SQLite) and serve content-addressed Hash/Cid types; `zim-crypto` is the only crate `zim-wasm` depends on. Keeping them separate isolates the wasm-target surface to one tiny crate. |
| `crdt` | No equivalent — Zim has no collaborative editor | The path-op CRDT inside `zim-fs` is for filesystem-merge conflict resolution, not for live editor state. No `zim-crdt` crate. |
| `app` | `zim-peer` + `zim-hub` (two binaries) | Two deployment shapes (headless daemon vs gateway-with-embedded-peer) with different feature sets (FUSE in `zim-peer`, OAuth + Askama in `zim-hub`). Keeping them as separate binaries with shared library code in `zim-peer` is cleaner than feature-gating one massive crate. |
| (pack has no analogue) | `zim-protocol` | The P2P wire protocol is large enough to warrant its own home; isolating it makes the `zim-fs` and `zim-store` crates testable without iroh networking. |
| (pack has no analogue) | `zim-runtime` | Shared lifecycle plumbing between the two binaries — leaf crate, ~140 LOC. |
| (pack has no analogue) | `zim-wasm` | Browser-side artifact; `cdylib` for `wasm-pack`. Cannot live inside `zim-hub` because the `cdylib` crate-type and the hub's `[[bin]]` are incompatible in one crate. |

Net: 8 crates vs pack's 3. Justified by (a) the P2P/crypto/wasm surface that pack doesn't have, and (b) keeping the wasm-target surface as small as possible.

## Cut-over history

This layout shipped in three commits:

- `0e1eada` — initial cut-over from the legacy `jax-common` / `jax-daemon` / `jax-object-store` / `jax-desktop` workspace to the five core `zim-*` crates (T-009).
- `fdda0f4` — post-cut-over rename cleanup (scripts, install, README, wiki) (T-009 follow-up).
- `badcb2e` — `crates/zim-runtime/` extraction, `zim-hub` embed-peer pivot, `zim-wasm` member registration, FUSE/sync shutdown discipline (T-007a + T-015 + T-012).

The original "current → target" mapping table and the eight-step cut-over sequence that used to live in this doc are no longer load-bearing — the cut-over happened. Anything that should remain about *how* the cut-over was done belongs in commit messages or the closed task notes (`.coord/tasks/done/T-009.md`, `T-007a.md`, `T-015.md`).

## What this doc deliberately does not cover

- **Internal API redesign inside any crate.** Per-crate module layouts evolve; see `docs/PROJECT_LAYOUT.md`.
- **HTTP surface details.** `zim-peer`'s API is in `docs/API.md`; `zim-hub`'s views and actions are documented in `crates/zim-hub/README.md`.
- **Database schema.** Lives next to the crates that own it (SQLite migrations in `zim-peer/migrations/` and `zim-hub`'s identity DB).
- **Per-task design history.** Closed tasks in `.coord/tasks/done/`.
- **Feature gates.** `zim-peer`'s `fuse` feature, `zim-crypto`'s `iroh-keys`/`wasm` features — documented at the crate level, not here.
```

---

## Coordination flags for you

1. **No "core" naming** — kept the rule in the no-no list per user's earlier insistence ("no do not call the crate core please be more fucking specific"). Even though pack uses `core`, the rule survives because Zim isn't collapsing to pack's shape.
2. **The pack-comparison table** is the substantive new content. If orch wants the rationale shorter, drop the table and keep only the one-paragraph "8 crates vs pack's 3" net.
3. **The "Cut-over history" section** intentionally reduces the old mapping/sequence sections to a 3-bullet pointer at commits. If you'd rather preserve the historical mapping table for archeology, append it as an appendix at the bottom.

I've marked T-010 as complete from my end pending your application of the doc. Move T-010 → `tasks/done/` when you've applied it; happy to make further edits if you want changes.
