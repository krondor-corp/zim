# zim-hub

Read-only web mirror gateway for Zim. Server-rendered Askama templates with Datastar for hypermedia (no SPA, no JS toolchain). **Embeds the peer in-process** — no external `zim-peer` daemon required.

## What this is

zim-hub is one of the two binary crates in the Zim workspace (the other is `zim-peer`, the headless system daemon + CLI + FUSE). zim-hub serves a **read-only** browser view over a **single-user** Zim workspace.

Per T-016 (in flight), zim-hub acts as a **mirror peer** at the protocol layer: it holds blobs and serves the **published-set** (T-008 — files/folders explicitly marked public) without holding bucket secrets. The peer runs in-process under zim-hub's runtime; one binary, one command.

Google OAuth gates access (planned, T-001). Single-user only; no multitenancy.

## Status: T-015 landed — embedded peer

The hub now boots its own peer Service in-process. The old `ZIM_HUB_PEER` env var and the reqwest-to-localhost code path are gone. Bucket browsing reads directly from the embedded `Database` + `Fs`. M5 (OAuth), M6 (Apalis), M7 (MCP), M4a (zim-wasm wiring) are the remaining T-002 milestones.

## How to run

**Canonical (hot reload):**

```
make hub
```

Starts the hub on `http://localhost:8080` with `cargo watch` on `src/`, `templates/`, `static/`, and `Cargo.toml` — edit a template, see the page reload. Prints the bound URL before exec. Requires `cargo-watch` (install once with `cargo install cargo-watch`).

The embedded peer creates its data directory on first launch (default `./data/zim-hub/`) containing:
- `zim-hub.db` — SQLite database (bucket log + migrations applied)
- `blobs/` — iroh-blobs store
- iroh node identity + DHT state

Defaults baked into `make hub`:
- `ZIM_HUB_LISTEN=127.0.0.1:8080` (override with `HUB_PORT=9000 make hub` or by exporting `ZIM_HUB_LISTEN` directly)
- `ZIM_HUB_DATA=./data/zim-hub`
- `RUST_LOG=info,zim_hub=debug`

Copy `.env.example` at the repo root to `.env` and `source .env` (or use direnv) to override.

**Fallback (no hot reload):**

```
cargo run -p zim-hub
```

Same defaults, no watcher. Useful when `cargo-watch` isn't available or you want a single run.

## Mirroring a bucket

zim-hub's embedded peer acts as a **mirror peer** — it holds blobs and serves a bucket's published-set without holding the bucket secret. To put a bucket onto a hub, the bucket's owner pre-authorizes the hub's peer key.

**On first boot, zim-hub prints a copy-pasteable command:**

```
─────────────────────────────────────────
  zim-hub v0.1.0
  listen   127.0.0.1:8080
  data     ./data/zim-hub
  node     1ea75079a6bc194f4b3e28dad40b49c8762ae0832fcba25ff043c1ff7f7ced81
  services http, peer (in-process / mirror)
─────────────────────────────────────────

To mirror a bucket on this hub, run on the owning peer:
  zim bucket mirror add <BUCKET_ID> 1ea75079a6bc194f4b3e28dad40b49c8762ae0832fcba25ff043c1ff7f7ced81
```

Copy the line into a shell on the owning peer (the machine running `zim` as a member of the bucket), substituting the bucket id you want to mirror. The owner's peer adds the hub's node id to the bucket's mirror set; the hub then fetches the published-set blobs and surfaces them at `/b/{BUCKET_ID}/tree/*`.

> **Status:** as of T-016d the banner is wired and the deploy flow is documented. The `zim bucket mirror add` CLI command itself lands under T-016b (thing1), and the protocol-level mirror-peer-type identification lands under T-016a (thing2). Until those land, the banner is informational — the command is the eventual interface. The hub still boots and serves its own data; it just won't sync remote buckets without the protocol-level mirror plumbing.

The hub's node id is stable across restarts as long as the `data` directory persists — the iroh secret key is stored there. Delete or rename the `data` directory and you get a new node id (and need to re-mirror).

## Environment

| Var | Default | Effect |
|---|---|---|
| `ZIM_HUB_LISTEN` | `127.0.0.1:8080` | HTTP bind address. |
| `ZIM_HUB_DATA` | `./data/zim-hub` | Data directory for the embedded peer (SQLite + blob store + node identity). Created on first launch. |
| `ZIM_HUB_LOG` | `info` | zim-hub-only tracing level. Overridden by `RUST_LOG`. |
| `RUST_LOG` | (unset) | Full tracing-subscriber filter. Overrides `ZIM_HUB_LOG`. |

## Architecture

```
            ┌────────────────────────── zim-hub binary ──────────────────────────┐
            │                                                                    │
   browser  │   axum (HTTP) ─── Askama templates ─── Datastar (client)           │
   ◀──────▶ │       │                                                            │
   :8080    │       └── PeerClient ── ServiceState ── Peer (in-process)          │
            │                          │                                          │
            │                          ├── Database (SQLite)                      │
            │                          └── Blobs   (iroh-blobs)                   │
            │                          └── DHT + relay                            │
            │                                                                    │
            └────────────────────────────────────────────────────────────────────┘
```

Both the HTTP server and the peer run as `Service` impls under the same `ShutdownHandle` (from `zim-runtime`). SIGINT drains both cleanly.

## Aesthetic / pattern reference

`https://github.com/krondor-corp/pack` — folder layout (`state.rs`, `http/{html,sse,health}/`, `templates/{layouts,pages,partials}/`, `static/`), `Service` trait via `zim-runtime`, handler-per-file under `http/html/<area>/{views,actions}/`. Hypermedia divergence: pack uses HTMX, zim-hub uses **Datastar**.

## Pattern decisions (per T-002 acceptance)

| Pattern | Decision | Rationale |
|---|---|---|
| `runtime::Service` trait | **Yes** | Lives in `crates/zim-runtime/` (T-007a). Both zim-hub and zim-peer spawn services under it. |
| Handler-per-file under `views/`/`actions/` | **Yes** | `http/html/bucket/views/{tree,blob,raw,history}.rs` is the live split. |
| Askama templates + Datastar | **Yes** | Binding per T-002/T-003. |
| Vendored single-file JS (no toolchain) | **Yes** | `static/vendor/datastar.min.js` + `static/vendor/zim-wasm/{zim_wasm.js, zim_wasm_bg.wasm}` are committed; bumped via vendor-only PR. |
| Embedded peer Service | **Yes** | T-015. No external daemon; one binary, one command. |
| `struct-patch` on models | **Later** | No models yet. Adopt when the first hub-side read-model shows partial-update pressure. |
| Apalis background jobs | **Later** | First job (snapshot index prep / full-text index of the published set) will trigger adoption. |
| MCP endpoint | **Later** | Lands as a sibling `Service` once the read-only tool catalogue is defined. |
| zim-hub-side SQLite for hub state | **Later** | Hub state (session, OAuth tokens) lands with the auth milestone. The peer's SQLite (bucket log) is already wired via the embedded `ServiceState`. |

## Layout

```
crates/zim-hub/
├── Cargo.toml
├── README.md (this file)
├── src/
│   ├── main.rs                 — init logging, build embedded peer state, spawn peer + http services
│   ├── lib.rs                  — module declarations + re-exports
│   ├── config.rs               — Config + env loader (ZIM_HUB_LISTEN, ZIM_HUB_DATA, ZIM_HUB_LOG)
│   ├── state.rs                — AppState (clone-cheap, holds PeerClient)
│   ├── errors.rs               — Error / Result, axum IntoResponse
│   ├── peer_client.rs          — In-process wrapper around the embedded zim-peer ServiceState
│   └── http/
│       ├── mod.rs              — HttpServer Service impl, build_router
│       ├── html/
│       │   ├── mod.rs          — html router
│       │   ├── index.rs        — GET /
│       │   ├── static_files.rs — GET /static/* via rust-embed
│       │   └── bucket/
│       │       ├── mod.rs      — /b/{id}/* router
│       │       └── views/
│       │           ├── mod.rs      — breadcrumb helper
│       │           ├── tree.rs     — GET /b/{id}/tree/*
│       │           ├── blob.rs     — GET /b/{id}/blob/*
│       │           ├── raw.rs      — GET /b/{id}/raw/*
│       │           └── history.rs  — GET /b/{id}/history
│       ├── health/
│       │   └── mod.rs          — GET /_status/livez | readyz | version
│       └── sse/
│           └── mod.rs          — stub for future Datastar SSE streams
├── templates/
│   ├── layouts/base.html
│   ├── partials/breadcrumb.html
│   └── pages/{index, bucket/{tree, blob, history}}.html
└── static/
    ├── style.css
    └── vendor/
        ├── README.md
        ├── datastar.min.js
        └── zim-wasm/{zim_wasm.js, zim_wasm_bg.wasm, ...}
```

## Vendoring policy

`static/vendor/` holds committed JS / WASM dependencies. See `static/vendor/README.md` for inventory and per-bundle bump procedures.
