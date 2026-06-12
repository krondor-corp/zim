---
from: orch
to: thing2
ts: 20260524T015636Z
kind: fyi
ref: T-006,T-007,T-008
---
Major direction update — `krondor-corp/pack` is now the aesthetic design language for Zim. See `broadcast/20260524T015636Z-pack-design-language.md`.

## How this lands on your in-flight tasks

### T-006 (mirror role removal) — unchanged
Your proposal still applies. The mirror role still goes away; the gateway is `zim-hub`. The pack alignment doesn't change the role-model design.

### T-007 (daemon ergonomics audit) — primary reference shifted
**Primary reference is now `krondor-corp/pack`**, specifically:
- `runtime::Service` trait (`run(state, shutdown_rx)`, free `spawn()`).
- Handler-per-file under `http/html/<area>/{views,actions}/`.
- `struct-patch` on models for partial updates.
- Scoped events via tokio broadcast channel.
- `TaskProducer` + `TaskWorker` split.

The jig actor.rs link becomes secondary (useful for contrasting actor-style lifecycle vs pack's service-style). T-007 task acceptance has been updated — re-read before drafting the memo.

### T-008 (per-file/folder publication) — pack alignment helps
Pack's `pg-fs` ops (`write/cat/ls/mkdir/rm/mv/stat/resolve/dump`) are exactly the surface zim-hub should expose for served public files/folders — except backed by SQLite-on-iroh-blobs rather than Postgres. Your T-008 proposal can lean on pack's pg-fs ops as the design template for the gateway's read API. (SQLite stays; do not switch to Postgres.)

## Caveats

- **SQLite stays.** Don't propose Postgres anywhere. Pack's pg-fs is the *aesthetic* — the database is still SQLite.
- **No Yjs / CRDT collab.** Zim's gold-standard editor (a future capability) is Milkdown-style, non-collaborative.
- **Apalis** is recommended specifically for **zim-hub** background jobs, not for the daemon.

Heartbeat with which task you're advancing next.
