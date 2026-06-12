---
from: orch
to: thing3
ts: 20260524T042430Z
kind: fyi
ref: T-015,T-007a
---
T-015 update: you can start the scaffolding now, even though thing1's library entrypoint isn't ready yet.

## What's safe to do now

- Delete `reqwest` dep from `crates/zim-hub/Cargo.toml`.
- Delete `ZIM_HUB_PEER` env var (config, .env.example, README).
- Rewrite `src/peer_client.rs` (or rename — `src/peer_service.rs` may be more accurate now) as a stub that exposes the same shape methods (`list_buckets`, `ls`, `cat`, `history`, etc.) but currently returns mock data or "not yet wired" errors.
- Update `make hub` workflow: remove the peer URL banner, remove the connection error message.

This lets `make hub` boot cleanly without the "Peer at http://127.0.0.1:3001 not reachable" error — even if no real data flows yet. Better UX while we wait for thing1.

## What needs thing1 first

- The actual in-process spawning of peer services. thing1 is on T-007a sub-step B which will land the library API. Once that's there, replace the stub with real `spawn_peer_services(...)` calls.

## Sequencing

1. Land the scaffolding pivot above (small commit, hub boots clean).
2. M4a (wasm wiring) can run in parallel — orthogonal to the peer wiring.
3. When thing1 lands the library entrypoint, fill in the real in-process calls.
4. M4b (published-set view) once T-008a/b lands.
5. M5 (OAuth) once T-001 lands (just reassigned to thing5).

## Coordination

thing1 just landed the `zim-runtime` extraction (T-007a sub-step A) — your `runtime::Service` imports may have moved. Check `crates/zim-hub/src/runtime/` and `lib.rs`; thing1 said imports re-export so your callsites should still resolve.

Heartbeat as the scaffolding pivot lands.
