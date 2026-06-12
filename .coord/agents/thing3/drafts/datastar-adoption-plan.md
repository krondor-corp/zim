# Datastar adoption plan for zim-hub

**Author:** thing3 (T-003)
**Policy:** clean-break (`broadcast/20260524T014147Z-clean-break-policy.md`) + pack-aesthetic (`broadcast/20260524T015636Z-pack-design-language.md`, clarified at `broadcast/20260524T015900Z-pack-is-aesthetic-only.md`); single-user; no HTMX; no Yjs.
**References (primary):** <https://github.com/krondor-corp/pack> — aesthetic and layout reference (folder layout, docs/wiki shape, Askama templates, server-rendered no-SPA feel). Zim **diverges from pack on hypermedia**: pack uses HTMX, Zim uses **Datastar**.
**References (secondary):** <https://data-star.dev/> (the Datastar library itself), <https://generic.krondor.org/>, <https://github.com/amiller68/thumbs-up>.
**Scaffolding owner:** T-002 (zim-hub crate). This doc is the **plan**: opinionated choices, integration touchpoints, and a no-HTMX rule. T-002 takes the plan and produces the buildable shell.

## 1. What Datastar is, in one paragraph

Datastar is a hypermedia framework: a tiny client-side runtime (~14 KB gzipped) that reads `data-*` attributes on server-rendered HTML and reacts to **server-sent events (SSE)** that merge HTML fragments into the DOM. It does for HTML what htmx does — server returns HTML, client patches the DOM — but with built-in **signals** (reactive client state), **actions** (declarative requests), and a single transport (SSE) for both initial loads and live updates. It replaces htmx, Alpine.js, and most front-end framework state in one dependency. No build step required.

## 2. Hard rules (no exceptions)

1. **No HTMX.** Anywhere. Not in templates, not in docs, not in fallback paths. Datastar is the only hypermedia layer.
2. **No npm/pnpm/Vite/Bun.** zim-hub has zero JavaScript build toolchain. The Datastar JS bundle is a single `.js` file served as a static asset (or vendored under `crates/zim-hub/assets/vendor/`).
3. **No SolidJS, React, Vue.** Server-rendered Askama templates only. Signals/actions live in `data-*` attributes.
4. **No client-side router.** Standard `<a href>` navigation. Datastar SSE handles live updates within a page.
5. **Single-user.** No tenant prefix on routes. No org/team UI. Auth state is single-session.
6. **Read-only.** zim-hub never writes to the bucket. All mutations remain on `zim-peer`.

## 3. Stack

| Layer | Choice | Already in workspace? |
|---|---|---|
| HTTP server | `axum 0.7` | yes |
| Template engine | `askama 0.12` + `askama_axum 0.4` | yes |
| SSE transport | `axum::response::sse::Sse` | yes (via axum) |
| Static assets | `tower-http` `ServeDir` / `ServeFile` | yes |
| Auth | Google OAuth2 (T-001 scope) — `oauth2` crate, deferred to T-002 | no (new dep) |
| Datastar client | Single `datastar.js` bundle vendored under `crates/zim-hub/assets/vendor/datastar.js` | n/a (no build) |
| CSS | Hand-written CSS in `assets/style.css`, OR a single utility-class file. **No Tailwind build.** Reference: krondor.org is plain CSS. | n/a |

Zero new heavy dependencies beyond what `crates/daemon` already pulls in. The plan deliberately reuses askama because it's already vetted in `crates/daemon/src/http_server/gateway/` (compiled templates).

## 4. Crate skeleton (proposed to T-002)

```
crates/zim-hub/
├── Cargo.toml
├── assets/
│   ├── style.css
│   ├── favicon.svg
│   └── vendor/
│       └── datastar.js          # one file, vendored, version-pinned
├── src/
│   ├── main.rs                  # binary `zim-hub`
│   ├── lib.rs                   # pub fn serve(config) -> ShutdownHandle
│   ├── config.rs                # listen addr, peer endpoint, oauth client
│   ├── peer_client.rs           # reqwest client → zim-peer HTTP API (read-only)
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── google.rs            # OAuth2 PKCE flow
│   │   └── session.rs           # signed cookie session, single-user
│   ├── routes/
│   │   ├── mod.rs               # axum::Router builder
│   │   ├── index.rs             # GET /
│   │   ├── bucket.rs            # GET /{bucket}, /{bucket}/tree/*, /{bucket}/history
│   │   ├── blob.rs              # GET /{bucket}/blob/*, /{bucket}/raw/*
│   │   ├── publish.rs           # GET /{bucket}/published   (T-008 published-set)
│   │   └── sse.rs               # GET /sse/{bucket}         (Datastar merge frames)
│   ├── templates/
│   │   ├── layout.html
│   │   ├── index.html
│   │   ├── bucket_tree.html
│   │   ├── bucket_history.html
│   │   ├── blob_view.html
│   │   └── partials/
│   │       ├── breadcrumb.html
│   │       ├── tree_row.html
│   │       └── publish_badge.html
│   └── errors.rs
└── tests/
    └── smoke.rs                 # boots the binary, hits `/`
```

`pub fn serve(cfg) -> ShutdownHandle` matches the `zim-peer` pattern (see `docs/concepts/index.md`) so an embedding host can mount zim-hub as a sub-service in tests.

## 5. Datastar usage by template

A short pattern catalog — what zim-hub actually does with Datastar. (Concrete reference: krondor.org. Concrete example repo: thumbs-up.)

### 5a. Initial render: pure server HTML

```html
<!-- partials/tree_row.html -->
<li class="tree-row" data-on-mouseenter="$hover = '{{ entry.path }}'">
  <a href="/{{ bucket_id }}/tree/{{ entry.path }}">{{ entry.name }}</a>
  {% if entry.published %}
    <span class="badge">public</span>
  {% endif %}
</li>
```

No JS framework. No hydration step. `data-on-mouseenter` is Datastar.

### 5b. Live updates: SSE merge-fragment

zim-hub subscribes to its local peer's bucket-changed events (via a long-poll or websocket on the peer's API — depends on T-007 outcome). It re-renders the affected partial server-side and pushes it down an SSE stream as a Datastar `datastar-merge-fragments` event.

Template attribute:

```html
<ul id="tree-{{ bucket_id }}" data-on-load="@get('/sse/{{ bucket_id }}/tree')">
  {% include "partials/tree_list.html" %}
</ul>
```

Server route (axum):

```rust
async fn sse_tree(
    Path(bucket_id): Path<String>,
    State(ctx): State<Ctx>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = ctx.peer.bucket_change_stream(&bucket_id)
        .map(|change| {
            let html = render_tree(&change.bucket_id);
            Event::default()
                .event("datastar-merge-fragments")
                .data(format!("<ul id=\"tree-{}\">{}</ul>", change.bucket_id, html))
        });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

### 5c. Read-only "publish" view (T-008 interaction)

zim-hub displays the **published-set** for a bucket (per T-008): files/folders explicitly marked public. It does not show the full bucket tree if there is no public capability. The `publish.rs` route lists what the gateway is allowed to serve.

```html
<!-- bucket_tree.html shows only public entries -->
{% for entry in published_entries %}
  {% include "partials/tree_row.html" %}
{% endfor %}
```

## 6. Integration touchpoints with the rest of the workspace

| Touchpoint | What zim-hub does | Owner crate / task |
|---|---|---|
| Identity / key custody | Google OAuth unlocks a server-side encrypted private key for the gateway. zim-hub never sees the bucket secret. | `zim-crypto`, T-001 model |
| Peer access | zim-hub talks to a local `zim-peer` via the existing HTTP API. No new wire protocol. | `zim-peer` (T-005), API in `crates/daemon/src/http_server/api/v0/` today |
| Published-set discovery | Uses T-008's per-file/folder publication metadata to enumerate gateway-readable entries. | T-008 |
| Change notifications | SSE upstream from `zim-peer` (or polling fallback). Exact mechanism deferred to T-007 daemon ergonomics outcome. | T-007 |
| FUSE / mounts | **Not used by zim-hub.** FUSE lives on the peer. | n/a |
| Workspace dependency edge | `zim-hub → zim-protocol, zim-fs, zim-store, zim-crypto` (per `docs/CRATES.md`). No edge to `zim-peer`. | T-005 |

## 7. Boundary between zim-hub Rust server and any future WASM client

Default: **no WASM client.** The hub is server-rendered Askama + Datastar SSE end-to-end. This matches krondor.org and thumbs-up.

If a future task introduces a WASM client (e.g. for client-side blob decryption when the published envelope requires a viewer-held key), the boundary is:

- Server: serves HTML shells, raw encrypted blobs at `/{bucket}/raw/*`, and a `application/wasm` payload at `/assets/viewer.wasm`.
- Client (WASM): renders decoded content inside a `<div id="viewer">` injected by Datastar. The WASM module is loaded only on routes that need it; not part of the global runtime.
- No NPM build step even with WASM — `wasm-pack` builds a single `.js` + `.wasm` pair, vendored under `assets/`.

This is a **deferred capability**, not part of T-003 acceptance.

## 8. Concrete checklist for T-002 (zim-hub scaffold)

- [ ] `crates/zim-hub/Cargo.toml` declares: `axum`, `askama`, `askama_axum`, `tokio`, `tower-http`, `serde`, `serde_json`, `tracing`, `reqwest` (workspace versions); plus `oauth2`, `cookie`.
- [ ] Vendor `datastar.js` under `assets/vendor/datastar.js` with a `README.md` recording version + upstream commit hash. Update via vendor-bump PR only.
- [ ] `src/lib.rs` exports `pub async fn serve(cfg: Config) -> ShutdownHandle` mirroring `crates/daemon/src/lib.rs:start_service`.
- [ ] `src/main.rs` is a thin shim: parse `clap` config, call `serve(...)`, await ctrl-c, call `shutdown()`.
- [ ] First milestone: `GET /` returns Askama-rendered index referencing `/assets/vendor/datastar.js`. No bucket logic.
- [ ] Second milestone: `GET /sse/healthz` opens an SSE stream and emits a `datastar-merge-fragments` frame every 5s into a `#heartbeat` div. Proves the wire-up.
- [ ] Then T-002 owners can layer in bucket routes against the peer client.

## 9. Future editor surface (out of scope for T-003)

When zim-hub eventually grows an editor surface, the target is **Milkdown-style, non-collaborative** — no Yjs, no CRDT collab, no multi-cursor. Single-user direct edits. Reserved layout seam: `crates/zim-hub/src/routes/editor.rs` + `src/templates/editor/` (not created in v1). If Milkdown is adopted, vendor as a single bundled `.js` + single `.css` under `assets/vendor/` — still no npm build. Because hub is read-only by policy, the editor route would either (a) call out to a paired `zim-peer`'s mutation API, or (b) the editor moves into `zim-peer`'s own HTTP surface; T-002 or a future editor task picks the boundary.

## 10. Pack-aesthetic adoption notes

Per `broadcast/20260524T015636Z-pack-design-language.md` (clarified at `20260524T015900Z-pack-is-aesthetic-only.md`), pack is the aesthetic reference for layout, naming, and the server-rendered no-SPA feel — not a binding source of architectural patterns. What this plan adopts from pack:

- **Askama** server-rendered templates (binding — already chosen above).
- **Server-rendered + vendored single JS file + no JS toolchain** (binding — the no-npm policy in §2 is the pack feel).
- **`docs/` / `wiki/` / `crates/` repo shape** (binding — applies to the workspace at large, not just zim-hub; see T-011).
- **`views/` + `actions/` handler-per-file split inside `src/routes/`** (optional inspiration — T-002 may adopt for clarity, not required).

What this plan deliberately does NOT inherit from pack:

- HTMX (Zim's binding divergence — see frontmatter).
- `runtime::Service` trait, `TaskProducer`/`TaskWorker`, Apalis, MCP endpoint, `struct-patch`, scoped events, branch DBs — pack patterns that are *interesting* but explicitly demoted to "optional inspiration" per the clarification broadcast. T-002 designer picks what fits zim-hub; nothing here is required.

## 11. What is explicitly NOT in scope here

- OAuth implementation details (T-001).
- Per-file publication envelope crypto (T-008).
- Mirror-role removal (T-006).
- Final styling, dark mode, accessibility audit.
- Mobile-specific UX.
- Any "fallback for users without JavaScript" — Datastar requires JS; we accept that for the hub.
- Editor implementation (future capability — see §9).
