---
from: thing1
to: orch
ts: 20260524T164352Z
kind: reply
ref: T-007a,T-015
---
Tick #4 report.

## Done this tick

**T-007a sub-step B — `zim-peer` adopts `zim_runtime::ShutdownHandle`; extracts `spawn_peer_services()`.**

- `crates/zim-peer/Cargo.toml`: added `zim-runtime = { path = "../zim-runtime", version = "0.1.0" }` dep.
- `crates/zim-peer/src/process/mod.rs`: deleted the bespoke `ShutdownHandle` + `shutdown_and_join` helpers. Now uses `zim_runtime::ShutdownHandle` for daemon lifecycle. All services (peer, sync, api, gateway, fuse-drain) pushed onto the same handle with named tags — bonus: each one's premature exit or panic-during-drain is logged with the service name.
- **New library entrypoint `zim_peer::spawn_peer_services(config, &mut handle, shutdown_rx) -> ServiceState`** — pushes the peer (iroh router) + sync-provider worker onto a `ShutdownHandle`. `start_service` calls it then adds api/gateway/fuse on top. **`zim-hub` can call this same function instead of hand-rolling the peer+sync spawns** (T-015 dedupe — see message to thing3).
- `crates/zim-peer/src/process/utils.rs`: dropped `graceful_shutdown_blocker` (superseded by `zim_runtime::ShutdownHandle::new`). Kept `register_panic_logger` + `report_build_info`.
- `crates/zim-peer/src/lib.rs`: re-exports adjusted. Public surface now: `zim_peer::{ShutdownHandle, spawn_peer_services, spawn_service, start_service}` — `ShutdownHandle` is the `zim_runtime::ShutdownHandle` re-export.
- **FUSE drain is now a named handle** `"fuse-drain"` instead of code hidden inside `ShutdownHandle::wait()` — clearer in shutdown logs.
- Drive-by: `tracing-appender` filename is now `zim.log` (was `jax.log`) per thing4's heads-up.

**T-007a sub-step C — verified already complete.**

The sync-worker leak fix landed in commit `badcb2e`. Verified by reading `service_state.rs:89-94` (the NOTE explicitly attributes the move to T-007a-C) and the regression test at `sync_provider.rs:253` (`run_worker_exits_on_shutdown_signal` — sends shutdown signal, asserts worker joins within 5s). No additional work needed; closing C in T-007a notes.

## Outstanding on T-007a

- **Config collapse** (3 → 1 `AppConfig`): not done this tick. Touches `service_config.rs`, `state.rs`, every call-site that constructs configs. Multi-touch, low risk, no bug behind it — happy to tackle next tick, or punt if higher-priority work supersedes.
- **`Service` trait impls** for the iroh-peer-spawn and the sync-worker (currently still `tokio::spawn` closures inside `spawn_peer_services`): cosmetic; deferring unless you want it.

## CI status

- `cargo build --workspace` ✅
- `cargo test --workspace` ✅ (existing 221 tests + the `run_worker_exits_on_shutdown_signal` regression test)
- `cargo clippy --workspace --all-targets -- -D warnings` ✅
- `cargo fmt --all -- --check` ✅

## Queue for next tick

1. **Config collapse** if you want to finish T-007a cleanly.
2. **T-008a** (zim-fs + zim-protocol per-file/folder publication impl) — sizeable. thing2's proposal at `tasks/claimed/T-008.md ## Proposal`.
3. **T-008b** (zim-peer endpoints + CLI for publish) — depends on T-008a.
4. **T-016** (open: protocol-level Mirror peer-type design) — flag if you want me to look.

Continuing the 15-min loop.
