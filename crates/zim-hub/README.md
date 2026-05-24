# zim-hub

Read-only web mirror gateway for Zim. Server-rendered Askama templates with Datastar for hypermedia (no SPA, no JS toolchain).

## What this is

zim-hub is one of the two binary crates in the Zim workspace (the other is `zim-peer`, the system daemon). zim-hub serves a **read-only** view of a bucket's **published-set** — files/folders explicitly marked public per T-008's per-file/folder publication model. zim-hub **never holds the bucket secret**.

Single-user only. No multitenancy, no team UX. Google OAuth gates access.

## Status: v0 scaffold

This is the first-milestone scaffold per `T-002` and the design from `T-003`'s `datastar-adoption-plan.md`. It boots, serves `GET /` as a Datastar-wired index page, and serves embedded static assets (CSS + vendored `datastar.min.js`). Bucket browsing, history, the published-set view, OAuth, the peer client, and the MCP/SSE/Apalis seams are not yet wired — they land in subsequent milestones.

## How to run

**Canonical (hot reload):**

```
make hub
```

Starts the hub on `http://localhost:8080` with `cargo watch` on `src/`, `templates/`, `static/`, and `Cargo.toml` — edit a template, see the page reload. Prints the bound URL before exec. Requires `cargo-watch` (install once with `cargo install cargo-watch`).

Defaults baked into `make hub`:
- `ZIM_HUB_LISTEN=127.0.0.1:8080` (override with `HUB_PORT=9000 make hub` or by exporting `ZIM_HUB_LISTEN` directly)
- `ZIM_HUB_PEER=http://127.0.0.1:3001`
- `RUST_LOG=info,zim_hub=debug`

Copy `.env.example` at the repo root to `.env` and `source .env` (or use direnv) to override.

**Fallback (no hot reload):**

```
cargo run -p zim-hub
```

Same defaults, no watcher. Useful when `cargo-watch` isn't available or you want a single run.

## Environment

| Var | Default | Effect |
|---|---|---|
| `ZIM_HUB_LISTEN` | `127.0.0.1:8080` | HTTP bind address. |
| `ZIM_HUB_PEER` | `http://127.0.0.1:3001` | Base URL of the local `zim-peer` daemon the hub reads from. |
| `ZIM_HUB_LOG` | `info` | zim-hub-only tracing level. Overridden by `RUST_LOG`. |
| `RUST_LOG` | (unset) | Full tracing-subscriber filter. Overrides `ZIM_HUB_LOG`. |

## Aesthetic / pattern reference

`https://github.com/krondor-corp/pack` — folder layout (`runtime/`, `state.rs`, `http/{html,sse,health}/`, `templates/{layouts,pages,partials}/`, `static/`), `Service` trait with `ShutdownHandle`, handler-per-file under `http/html/<area>/{views,actions}/`. Hypermedia divergence: pack uses HTMX, zim-hub uses **Datastar**.

## Pattern decisions (per T-002 acceptance)

| Pattern | Decision | Rationale |
|---|---|---|
| `runtime::Service` trait | **Yes** | Adopted from pack. Clean lifecycle, free `spawn()`, makes adding the task worker / SSE pump trivial. |
| Handler-per-file under `views/`/`actions/` | **Yes (deferred form)** | The v0 scaffold has flat `http/html/{index,static_files}.rs`. As soon as the first area lands (`bucket/`), the `views/`/`actions/` split kicks in. |
| Askama templates + Datastar | **Yes** | Binding per T-002/T-003. Server-rendered HTML; Datastar reads `data-*` attributes and consumes SSE merge-fragment streams. |
| Vendored single-file JS (no toolchain) | **Yes** | `static/vendor/datastar.min.js` is committed; bumped via vendor-only PR. See `static/vendor/README.md`. |
| `struct-patch` on models | **Later** | No models yet. Adopt when the first read-model (e.g. `BucketView`) shows partial-update pressure. |
| Apalis background jobs | **Later** | v0 has no jobs. First job (snapshot index prep / full-text index of the published set) will trigger adoption. |
| MCP endpoint | **Later** | First-class per pack broadcast but not load-bearing for the v0 milestone. Lands as a sibling `Service` under `src/http/mcp/` once the read-only tool catalogue is defined (`list_buckets`, `ls`, `cat`, `history`). |
| SQLite for state | **Later** | The v0 scaffold has no state to persist. Hub state (session, OAuth tokens) lands with the auth milestone. |
| `peer_client` reqwest module | **Later** | The v0 milestone proves the wire-up; bucket browsing is the second milestone. |

The decisions table above will move into `docs/architecture/zim-hub.md` once thing4 lands the `docs/` reshape (T-011).

## Layout

```
crates/zim-hub/
├── Cargo.toml
├── README.md (this file)
├── src/
│   ├── main.rs                 — clap-free shim: build state, spawn services, wait
│   ├── lib.rs                  — module declarations
│   ├── config.rs               — Config + env loader
│   ├── state.rs                — AppState (clone-cheap)
│   ├── errors.rs               — Error / Result
│   ├── runtime/
│   │   └── mod.rs              — Service trait + ShutdownHandle (pack pattern)
│   └── http/
│       ├── mod.rs              — HttpServer Service impl, build_router
│       ├── html/
│       │   ├── mod.rs          — html router + wrap() helper + is_datastar()
│       │   ├── index.rs        — GET /
│       │   └── static_files.rs — GET /static/* via rust-embed
│       ├── health/
│       │   └── mod.rs          — GET /_status/livez | readyz | version
│       └── sse/
│           └── mod.rs          — stub for future Datastar SSE streams
├── templates/
│   ├── layouts/base.html       — html shell, loads /static/style.css + datastar.min.js
│   └── pages/index.html        — landing page
└── static/
    ├── style.css               — hand-written, no framework
    └── vendor/
        ├── README.md           — vendoring policy
        └── datastar.min.js     — placeholder; bump via vendor-only PR
```

## Vendoring policy

`static/vendor/datastar.min.js` is a committed copy of the upstream Datastar build. See `static/vendor/README.md` for the version, the source URL, and the bump procedure.
