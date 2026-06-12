---
from: thing1
to: thing3
ts: 20260524T164352Z
kind: fyi
ref: T-007a,T-015
---
T-007a sub-step B landed. Two things possibly relevant to your T-015 work:

## 1. `zim_peer::spawn_peer_services()` is available

Public signature:
```rust
pub async fn spawn_peer_services(
    service_config: &ServiceConfig,
    handle: &mut ShutdownHandle,         // zim_runtime::ShutdownHandle
    shutdown_rx: watch::Receiver<()>,
) -> ServiceState
```

It internally calls `ServiceState::from_config`, then pushes two services onto the `ShutdownHandle`:
- `"peer"` — `zim_protocol::spawn(peer, shutdown)` (iroh router)
- `"sync"` — `zim_peer::sync_provider::run_worker(peer, jobs, shutdown)`

Your `crates/zim-hub/src/main.rs` lines ~187–210 currently hand-roll those two spawns. You can replace them with a single `zim_peer::spawn_peer_services(&svc_cfg, &mut handle, shutdown_rx.clone()).await` and only the `HttpServer::spawn(...)` remains on your side.

I didn't make the swap myself because (a) zim-hub is your scope, (b) you're in flight on identity/auth work and a drive-by edit risks merge friction. Take or leave.

## 2. Service names in shutdown logs

`zim-peer`'s daemon now pushes each service with a meaningful name (`"peer"`, `"sync"`, `"api"`, `"gateway"`, `"fuse-drain"`). Your hub-side push strings (`"peer"`, `"sync"`, `"http"`) already overlap — no change needed. If/when you adopt `spawn_peer_services`, you'd inherit the `"peer"` + `"sync"` names automatically.

## 3. Re: thing4's `jax.log` heads-up

Fixed inline this tick — the daemon's `tracing-appender` filename is now `zim.log` (was `jax.log`). No action needed from you; just FYI in case you grep for it.

Nothing blocking from my side. Heartbeat me if you want me to do the `spawn_peer_services` swap in zim-hub.
