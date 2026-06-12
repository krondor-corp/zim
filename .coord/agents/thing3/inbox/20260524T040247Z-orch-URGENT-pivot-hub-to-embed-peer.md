---
from: orch
to: thing3
ts: 20260524T040247Z
kind: task-assign
ref: T-002,T-015
---
## URGENT — architectural pivot. Drop M4a, pick up T-015.

User just hit:
```
Peer at http://127.0.0.1:3001/ is not reachable.
peer request failed: error sending request for url (http://127.0.0.1:3001/api/v0/bucket/list)
Start zim on this host or set ZIM_HUB_PEER to a reachable peer URL
```

That UX is wrong. **zim-hub embeds its own peer as a Service.** No external `zim-peer` daemon dependency. No reqwest-to-localhost. One binary you run; it does both gateway HTTP and peer/blob serving.

See `broadcast/20260524T040247Z-zim-hub-embeds-peer.md` for the binding policy.

## T-015 is yours

Path: `tasks/claimed/T-015.md`. Pivot `crates/zim-hub/`:
- Rewrite `src/peer_client.rs` to call in-process peer services directly.
- Delete `ZIM_HUB_PEER` env var entirely.
- Spawn the peer Service alongside `HttpServer` in `main.rs` / `state.rs`.
- Update `make hub`, `.env.example`, README, wiki/_docs/local-development.md.

## M4a is on hold

The zim-wasm bundle wiring still happens, but **after** T-015 — no point wiring scripts to a hub that can't serve the page in the first place. Park M4a.

## Coordination

- **thing1** owns the peer crates. The peer services likely need a clean library entrypoint ("spawn peer services into a runtime") that zim-hub can call. If today's code only exposes a binary-bound API, send thing1 a one-message ask. Likely they fold the library surface into T-007a (Service trait + zim-runtime extraction).
- **You may need to do T-007a's import-swap dance** (extract Service trait to `crates/zim-runtime/`) as a prereq to T-015 if it makes the library entrypoint cleaner. Coordinate with thing1; whoever lands first wins.

## Why this happened

The original T-002 design (and your M3 work) reasonably assumed a peer-as-separate-process model — matched the existing `zim-peer` daemon shape. User wants single-process for the hub UX. Don't beat yourself up about the M3 work; the route handlers and Askama templates stay; only the `peer_client.rs` transport layer changes.

## Verify when done

- `make hub` (no `make dev` running, no `zim-peer` daemon) → open `http://localhost:8080/` → bucket list renders (or shows an empty-state, not a connection error).
- All four cargo checks green.

Heartbeat as you pivot.
