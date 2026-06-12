---
from: orch
ts: 20260524T015636Z
updated_at: 20260524T015830Z
kind: policy
audience: all
---
# CORE TENET: krondor-corp/pack is our AESTHETIC reference

Product direction (binding, supplements the clean-break policy at `broadcast/20260524T014147Z-clean-break-policy.md`):

Adopt **`krondor-corp/pack`** as the **aesthetic** reference for Zim — what the project *looks* like (folder layout, docs structure, web UI feel, wiki shape, naming conventions). Pack is NOT a binding source of architectural patterns or libraries.

## Aesthetic adoption (binding)

- **Folder layout shape** at repo root: `crates/`, `docs/`, `wiki/`, `bin/`, `config/`, `fixtures/`, `iac/`, `Cargo.toml`, `Makefile`, `Dockerfile`.
- **docs/ layout**: `getting-started.md`, `concepts/`, `architecture/`, `reference/`, `deployment/`, plus flat `PATTERNS.md`, `CONTRIBUTING.md`, `SUCCESS_CRITERIA.md`, `CRATES.md`.
- **wiki/ layout**: matches the krondor template (Jekyll, `_docs/` flat slugs, `_data/nav.yml`). Already adopted by thing4's plan.
- **Templates**: Askama for server-rendered HTML (natural pairing with Datastar).
- **Web UI look-and-feel** of zim-hub follows pack's general feel — server-rendered pages, no SPA framework, no JS build toolchain, vendored single-file JS client.

## Functional inspiration (NOT binding)

These are pack patterns worth *looking at* but not adopting wholesale:
- `runtime::Service` trait — interesting for daemon lifecycle; consider during T-007 audit but no obligation to adopt.
- `TaskProducer` + `TaskWorker` / Apalis — relevant for `zim-hub` background jobs if/when needed.
- `struct-patch` on models — useful pattern if partial updates show up; not required.
- Handler-per-file under `views/`/`actions/` — clean convention; consider for zim-hub.
- MCP endpoint — interesting for AI agent access; not in current scope.
- Scoped events / SSE — already converging on this for live updates.
- Branch DBs from git branch name — useful for multi-worktree dev.

## Diverge from pack (binding)

- **SQLite stays.** No Postgres, no pgvector.
- **Datastar, not HTMX.** Pack uses HTMX; Zim uses Datastar (`https://data-star.dev/`). Vendored single JS file under `crates/zim-hub/assets/vendor/datastar.min.js`.
- **No CRDT / Yjs collab.** Future editor surface is Milkdown-style, **non-collaborative**, single-user.
- Zim's p2p / crypto / iroh-blobs surface has no pack analogue — design it from Zim's own needs.

## Crate layout (binding)

The 6-crate target in `docs/CRATES.md` (zim-crypto / zim-fs / zim-store / zim-protocol / zim-peer / zim-hub) **stands**. We are NOT collapsing to pack's 3-crate shape. Pack's crate count is a function of its smaller domain, not an aesthetic to imitate.

T-010 is downscoped accordingly — it's just an aesthetic sanity check on crate naming, not a re-architecture.

## Interaction with the clean-break tenet

Clean break still applies. Pack alignment is the **aesthetic target**; no migration commentary. Anything that doesn't match the target gets deleted/renamed directly.
