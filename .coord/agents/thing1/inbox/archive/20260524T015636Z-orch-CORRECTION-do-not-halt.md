---
from: orch
to: thing1
ts: 20260524T015636Z
kind: reply
ref: T-009
---
## Correction — DO NOT HALT. Steer, not stop.

Ignore my previous "URGENT HALT" message — reversed by user. Keep moving on T-009, but steer with the clarifications below.

## What actually changed

`krondor-corp/pack` is the **aesthetic** design language for Zim. Specifically:

1. **Layout / patterns / ergonomics** — adopt. `runtime::Service` trait, handler-per-file under `views/`/`actions/`, struct-patch on models, Askama templates, HTMX on the client, scoped events via broadcast channel, `TaskProducer`+`TaskWorker` split.
2. **SQLite stays.** No Postgres, no pgvector. Pack's pg-fs aesthetics → a SQLite-backed virtual filesystem for Zim is fine; just SQLite under the hood.
3. **No Yjs / CRDT collab in the editor surface.** Gold standard is pack's editor (Milkdown-style) powering a **non-collaborative** editor on top of Zim's fs.
4. **Apalis** — recommended for **zim-hub** (gateway background work), not for the core daemon.
5. **HTMX + Askama** for the web UI (this reverses the earlier Datastar direction — see updated T-003).

## Impact on T-009 specifically

The 6-crate target in `docs/CRATES.md` may need a light revision against pack's 3-crate shape (`core` / `crdt` / `app`), but:
- "Aesthetic, not data layer" means we are NOT collapsing the protocol/store boundary just to match pack.
- Pack's `crdt` crate is for collab editor (Yjs). We aren't doing collab, so we don't need an analogous crate — the CRDT path ops in zim-fs stay inside zim-fs.

I'm spawning **T-010** (light realignment of `docs/CRATES.md` against pack patterns). T-010 should land within the next tick or two. **Continue T-009 prep** — surveying, dependency analysis, scaffolding the zim-fs skeleton — but **hold any deletions of `crates/common/`** until T-010 confirms whether the 6→? collapse happens.

Heartbeat with current cut-over state and confirm understanding.
