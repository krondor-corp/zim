---
from: orch
to: thing1
ts: 20260524T040247Z
kind: task-assign
ref: T-007a,T-015
---
## Architectural pivot — peer services need to be library-spawnable

User direction: zim-hub embeds its own peer as a Service. No more `zim-hub → reqwest → zim-peer` HTTP roundtrip. See `broadcast/20260524T040247Z-zim-hub-embeds-peer.md`.

## What this means for your queue

**T-007a (zim-runtime extraction + Service trait adoption + sync_provider leak fix) just became load-bearing.** Both binaries (`zim-peer` the headless daemon, `zim-hub` the embedded-peer gateway) use the same `Service` trait to spawn the same peer-side services.

Adjust T-007a acceptance to include:
- A **library entrypoint** in whichever crate hosts the peer services (`zim-protocol`? a new `zim-node`? your call) that exposes `spawn_peer_services(state, runtime) -> Vec<ServiceHandle>` or similar.
- `zim-peer/src/main.rs` calls this entrypoint to spawn its services. Currently they're spawned directly from `main.rs`; refactor so the spawning is in a library function.
- `zim-hub` will call the same entrypoint (via T-015).

## Sequencing

- **T-010** (CRATES.md aesthetic pass) — still do this first, quick win.
- **T-007a** — now bigger scope (extract zim-runtime + add library entrypoint for peer services + adopt Service trait + sync_provider leak fix). This unblocks T-015.
- **T-008a / T-008b** — still queued. T-008a/b can land in parallel with T-015 once T-007a's library entrypoint exists. T-008b's HTTP endpoints still matter for the standalone `zim-peer` daemon use case (headless / FUSE), but zim-hub bypasses them.

## What changes in the crate boundary

If the peer services (sync provider, blob serving, etc.) currently live in `zim-peer/src/`, they may need to move to `zim-protocol/src/` or a new `zim-node` crate so zim-hub can depend on them without a circular edge. Your call on the crate-boundary shape; document the decision in T-007a notes.

## Posture

T-015 (thing3) is waiting on your library entrypoint. Higher priority than T-008a/b. Hit T-010 quick, then T-007a.

Heartbeat as you start.
