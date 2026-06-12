---
from: orch
ts: 20260524T040247Z
kind: policy
audience: all
---
# CORE TENET: zim-hub embeds its own peer (a Mirror) as a Service

Product direction (binding, supplements clean-break + pack-aesthetic policies):

**zim-hub runs its own peer in-process as a Service. That peer IS a Mirror** — a non-member peer that holds blobs and serves the published-set without holding the bucket secret. No external `zim-peer` daemon dependency. No reqwest-to-localhost HTTP plumbing. One binary you run; it does both the gateway HTTP surface and the mirror peer.

## Clarification on the Mirror concept

T-006 removed `PrincipalRole::Mirror` from the share/membership model — that was right (T-008 handles publish access via per-entry secrets). But the protocol still needs a notion of **mirror peer**: a peer-network-layer identification, distinct from bucket membership. zim-hub is a Mirror in this sense.

T-016 (new) covers the protocol-level Mirror peer-type design. Acts as input to T-015 (zim-hub embedding) and T-007a (peer Service library entrypoint).

## Why

User just hit: `Peer at http://127.0.0.1:3001/ is not reachable. … Start zim on this host or set ZIM_HUB_PEER to a reachable peer URL`. That UX is wrong. The hub should not require a second process to be running for it to start.

## What changes

- **`crates/zim-hub/src/peer_client.rs`** is replaced (or rewritten) to call **in-process** peer services directly. No reqwest, no `ZIM_HUB_PEER` env var, no localhost HTTP roundtrip for the bucket-read path.
- **`crates/zim-hub`** gains a dep on whichever crates expose the peer surface (likely `zim-protocol` + `zim-fs` + `zim-store` + `zim-crypto`). zim-hub spawns the peer services under its own `Service` runtime alongside the `HttpServer`.
- **`crates/zim-peer`** stays the standalone daemon for people who want headless / CLI / FUSE without the web gateway. Its services should be **usable as a library** so zim-hub can spawn the same code in-process.
- **The two-process dev workflow** (`make dev` two-node tmux + `make hub` against `http://127.0.0.1:3001`) goes away for the gateway-side. `make hub` becomes a single-process command. `make dev` stays for multi-peer p2p sync testing.

## What stays the same

- zim-peer the binary is not deleted — it's the headless daemon use case.
- Datastar + Askama UI in zim-hub.
- T-008's per-file/folder publish surface — it just lives in the in-process peer instead of behind HTTP.
- T-001 identity/key model — same, just locally consumed.

## What gets re-derived from this

- **T-002**: scope expands. zim-hub now manages a peer Service, not just an HTTP relay. M3 (bucket views via peer_client) needs pivot to in-process calls. M4 (zim-wasm wiring) unaffected. M5 (OAuth) unaffected.
- **T-007a**: `crates/zim-runtime/` extraction becomes load-bearing — both zim-hub and zim-peer use the same `Service` trait to spawn the same peer-side services.
- **zim-peer library shape**: needs a clean "spawn peer services into a runtime" entrypoint that zim-hub calls. This may be a new task spawned off T-007a or T-008b.

## Sequencing

I'm spawning:
- **T-015** (thing3) — pivot zim-hub from external HTTP client to in-process peer Service.
- **Update T-007a** (thing1) — flag that zim-runtime needs to be the spawning surface for both binaries.
- **Hold T-008b** until the in-process surface is clear (the HTTP endpoints may still be needed for the standalone zim-peer daemon, but zim-hub doesn't go through them).

## Posture

If you're in flight on something that assumes external HTTP between hub and peer, pause and check with orch. If you're working on zim-peer or zim-fs/zim-protocol internals, no immediate impact — your services just gain a new in-process consumer.
