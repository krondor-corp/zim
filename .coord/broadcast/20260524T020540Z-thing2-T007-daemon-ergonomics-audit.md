---
from: thing2
ts: 20260524T020540Z
subject: T-007 daemon ergonomics audit (memo)
ref: T-007
---

# Daemon ergonomics audit (T-007)

Scope: `crates/zim-peer/**` — the daemon binary `zim` plus its library. (Task lists `crates/daemon/**` which is the legacy name; the present crate is `zim-peer`.)

Output format: prose audit + prioritized improvements list. Design-only. Recommendations are made on their own merits; pack (`krondor-corp/pack`) is the aesthetic reference and jig's `actor.rs` is the contrast.

## 1. Architecture map (current)

**Crate shape.**  Single binary `zim` + a library `zim_peer` (the binary is bin/main.rs, ~70 lines; everything else is library code). Workspace deps: `zim-crypto`, `zim-fs`, `zim-store`, `zim-protocol`. The peer-side iroh protocol lives in `zim-protocol` (separate crate); `zim-peer` is the "everything else" host process.

**Module structure** (`crates/zim-peer/src/`):
- `main.rs` — Args parse, OpContext build, CLI dispatch. The binary always runs as a *client* and dispatches commands; one of those commands (`Daemon`) starts the actual daemon services.
- `cli/` — Op pattern. `args.rs` + `op.rs` + `ops/` per-command modules. Commands are joined via the `command_enum!` macro. CLI talks to the running daemon over HTTP through `cli::op::OpContext::api_client`.
- `http_server/` — Two parallel routers (API on `api_port`, gateway on `gateway_port`) plus `health`, `handlers`, `gateway/{cache,directory,file,index,rewrite,transform,version}`, `api/{client,v0}`. Static assets via `rust-embed`.
- `process/` — Lifecycle entry points: `start_service`, `spawn_service`. Spawns peer + API + gateway + (with `fuse`) auto-mount as four independent `tokio::spawn` blocks sharing a `watch::Receiver<()>` shutdown channel.
- `service_state.rs` — `State` (a.k.a. `ServiceState`) holds `Database`, `Peer<Database>`, optional `Arc<RwLock<Option<MountManager>>>`. Built by `State::from_config`.
- `service_config.rs` — `Config`, the runtime knobs the process needs (paths, ports, log level, gateway URL).
- `state.rs` — `AppConfig` and `AppState`, the *on-disk* configuration (TOML at `~/.jax/config.toml`, key.pem, blobs dir, sqlite path). The lifecycle layer above converts `AppConfig` → `ServiceConfig` → spawns servers.
- `sync_provider.rs` — `QueuedSyncProvider`: a flume-channel-backed task queue for sync jobs, with a worker function `run_worker` spawned bare (no `Service` wrapper, no shared shutdown signal).
- `blobs/`, `database/`, `fuse/` (feature-gated), `clone_state.rs`, `version.rs` — supporting modules.

**Command/HTTP duplication.** Every operation has two surfaces:
1. CLI Op in `cli/ops/<area>/<verb>.rs` returning typed data, rendered via `Display` in the binary.
2. HTTP handler in `http_server/api/v0/<area>/<verb>.rs` returning JSON.

In the current code the CLI calls the daemon's HTTP API (via `OpContext::api_client`), so the CLI Op is effectively a thin HTTP-client wrapper plus formatting. The duplication is in argument marshalling and response decoding, not in domain logic.

**Lifecycle.** `process::start_service`:
1. `utils::graceful_shutdown_blocker()` → `(graceful_waiter, shutdown_tx, shutdown_rx)`.
2. `ServiceState::from_config(...)` (which itself spawns the sync_provider worker as a side effect; see pain point §2-3).
3. Three independent `tokio::spawn` blocks: peer (`zim_protocol::spawn`), API server (`http_server::run_api`), gateway server (`http_server::run_gateway`). Each clones `shutdown_rx`, has its own error-log path.
4. (`fuse` only) Spawns a delayed mount-manager start.
5. Returns `(state, ShutdownHandle)` to the caller. `spawn_service` is `init_logging + start_service + handle.wait()`.

**Shutdown.** `ShutdownHandle::wait` first stops FUSE mounts (with `fuse`), then `shutdown_and_join` awaits the graceful waiter, then `join_all(handles)` with a 30-second timeout. `ShutdownHandle::shutdown()` just sends `()` on the watch channel.

## 2. Pain points

### P-1 — Three parallel lifecycle bootstrap blocks
`process::start_service` has three near-identical `tokio::spawn` blocks for peer, API, and gateway, each cloning `shutdown_rx`, each with its own bespoke error-log path. Adding a fourth long-running service (e.g. an MCP endpoint per the pack broadcast, or a background indexer) means a fourth copy of this pattern. The shape "spawn a thing that listens, watch a channel, log errors, push handle into a Vec" repeats verbatim.

**Suggested improvement.** A `Service` trait:

```rust
#[async_trait]
pub trait Service: Send + 'static {
    async fn run(self: Box<Self>, state: AppState, shutdown_rx: watch::Receiver<()>) -> anyhow::Result<()>;
    fn spawn(self, state: AppState, shutdown_rx: watch::Receiver<()>) -> JoinHandle<()>
    where Self: Sized { /* default impl: tokio::spawn(self.run(...)) with error-log */ }
}
```

`start_service` becomes a vec of `Box<dyn Service>` that are spawned uniformly. Each long-running thing is one impl block. Pack uses this exact pattern in its `runtime::Service`. The win is concrete: ~60 LoC deleted from `process/mod.rs` and an obvious place to plug new services (MCP, indexer, hub jobs) without further bootstrap edits.

Caveat: the daemon's `zim_protocol::spawn(peer, rx)` is not under this crate's control — `zim-protocol` exposes its own lifecycle. A `Service` impl in `zim-peer` would wrap that call. Pack's pattern survives this wrapping cleanly.

### P-2 — Sync provider worker bypasses the shutdown signal
In `service_state.rs:81-87`, the QueuedSyncProvider worker is spawned bare with `tokio::spawn` inside `State::from_config`. The handle is dropped on the floor and the worker never sees the daemon's `shutdown_rx`. On daemon shutdown, the API/gateway/peer tasks shut down within the 30s timeout, but the sync worker keeps running until the process actually exits.

This is a real lifecycle bug, not just an aesthetic issue. The fix is structural: the sync worker must be one of the services spawned by the lifecycle layer (P-1), not an unmanaged side effect of state construction.

**Suggested improvement.** Move sync worker spawn out of `State::from_config`. Have `QueuedSyncProvider` (or a new `SyncService`) implement the `Service` trait; let `process::start_service` spawn it like any other service.

### P-3 — Three configuration types for one config
- `state::AppConfig` — on-disk TOML, fields like `api_port`, `gateway_port`, `peer_port`, `blob_store`, `max_import_size`.
- `service_config::Config` — runtime-only `ServiceConfig`, holds paths, ports, gateway URL, log level, node listen addr, node secret, sqlite path, jax_dir, max_import_size.
- `http_server::Config` — per-server `Config` with listen_addr + gateway_url + log_level.

There's overlap: ports appear in both `AppConfig` and `ServiceConfig`; `max_import_size` is in both; `gateway_url` shows up in `ServiceConfig` and `http_server::Config`. The conversion is hand-coded and easy to drift out of sync.

**Suggested improvement.** Single `AppConfig` as the source of truth (TOML on disk). Server-specific views are *projections* (e.g. `cfg.api_server_config()`), not separately-typed-and-owned. `struct-patch` (pack uses this) can give us partial-update ergonomics if/when we add an HTTP `PATCH /config` surface.

### P-4 — Mount manager mutability dance
`mount_manager: Arc<RwLock<Option<MountManager>>>`. Three layers because the manager is constructed *inside* `State::from_config` (so the outer `Arc<RwLock<Option<_>>>` initially holds `None` and is mutated to `Some` later in the same function). Every reader takes the read lock and matches on `Option`.

The `Option` exists only to express "not yet built." Once built, it never becomes `None` again. The lock exists only because of the init-time mutation.

**Suggested improvement.** Build the mount manager before constructing `State`. Hold it as `Arc<MountManager>` — no lock, no `Option`. This pairs with the broader move (P-1) toward `Service`-style construction where setup is explicit and ordered.

### P-5 — Two configuration loading paths (init vs load)
`AppState::init` writes a brand-new state directory; `AppState::load` reads an existing one; the two have parallel "verify files exist" / "create directories" branches. The set of files (`db.sqlite`, `key.pem`, `blobs/`, `config.toml`) lives in both.

**Suggested improvement.** A single `Layout` type that owns the file-set definition and exposes `Layout::exists()`, `Layout::create()`, `Layout::load()`. Both `init` and `load` go through it.

### P-6 — No event channel for observable state changes
There's no `tokio::sync::broadcast::Sender<Event>` in the daemon. Sync progress, new versions arriving, mount state changes, gateway cache hits/misses — none of these are observable as events. Anyone wanting "live progress" has to poll the database. A CLI `zim watch` or an SSE endpoint can't exist without retrofitting publish points throughout the code.

**Suggested improvement.** Add `Events { tx: broadcast::Sender<Event> }` to AppState. Define a typed `Event` enum scoped (`Event::SyncProgress { bucket_id, … }`, `Event::ManifestChanged { bucket_id, … }`, `Event::MountStateChanged { … }`). Sites that mutate state emit events; SSE handler filters by scope. Pack does exactly this with `User(Uuid) | Broadcast` scopes; ours would be `Bucket(BucketId) | Mount(MountId) | Broadcast`.

This is a moderate-effort retrofit (every commit point in the protocol layer needs an emit call) but it unlocks the entire observability story.

### P-7 — No `tasks/` domain for background jobs
QueuedSyncProvider is the only background-job pattern and it's bespoke (flume channel + raw worker function). The pack broadcast calls out a `TaskProducer + TaskWorker` split with task types organized under `tasks/<area>/`. As soon as we add a second background job kind (indexing, retention sweep, hub re-publish), we'll re-invent QueuedSyncProvider.

**Suggested improvement.** Generalize sync_provider into a `Tasks` module: typed `Task` enum (or trait), `TaskProducer` as the AppState-level client, `TaskWorker` as a `Service` impl. Apalis (called out in the broadcast for zim-hub specifically) is one library candidate but the simpler in-house version is fine here too; SQLite-backed persistence is enough for the daemon's job set.

### P-8 — Logging init mixed with service spawning
`init_logging` runs inside `spawn_service`, ahead of `start_service`. Embedded callers (Tauri's desktop crate, integration tests) who already have their own logger end up with conflicting `tracing_subscriber::registry().init()` calls.

**Suggested improvement.** `init_logging` exported separately. `spawn_service` documented as "convenience for CLI binary use only"; embedded callers call `start_service` after their own logging setup. Pack separates these.

### P-9 — `AsRef` impls obscure what `State` is
`impl AsRef<Peer<Database>> for State` and `impl AsRef<Database> for State` let handlers take `impl AsRef<…>` and pretend State is whatever they want. The cost is that reading a handler signature tells you nothing about what it actually needs — could be the peer, could be the database, could be both. Removing the impls forces handlers to declare their dependencies, which is what we want from a code-as-documentation perspective.

**Suggested improvement.** Drop both `AsRef` impls. Handlers take `&AppState` explicitly and call `.peer()` / `.database()`. The signatures get longer in trivial ways and clearer in non-trivial ways.

### P-10 — `ShutdownHandle` couples programmatic shutdown to a handle object
`ShutdownHandle::shutdown(&self)` sends on a `watch::Sender<()>` that the handle owns. To shut down programmatically you must hold the handle. Pack's pattern returns the `Sender` directly (or surfaces a typed `ShutdownToken`) so the caller can hand it to subsystems that need to trigger shutdown without coupling them to the lifecycle bookkeeping.

**Suggested improvement.** Expose `ShutdownToken(watch::Sender<()>)` as a first-class type; `start_service` returns it alongside the join handles. Smaller change than the others; mostly aesthetic.

## 3. Comparison: pack `runtime::Service` vs jig actor

**Pack `runtime::Service`** (the broadcast's primary reference for ergonomics).
- One trait, `async fn run(state, shutdown_rx)`, default `spawn()`. Stateless service objects.
- Fits the Zim daemon's shape well: peer, API server, gateway server, sync worker, future MCP endpoint, future hub re-publisher — all are "run a loop, listen to shutdown, log on error."
- Doesn't conflict with `zim-protocol::Peer` having its own internal loop; the Service wraps it.
- Improvements P-1, P-2, P-7, P-8 all land cleanly under this trait.

**Jig `actor.rs`** (single-actor, typed-inbox pattern).
- Actor owns state; receives typed messages on a channel; mutates state in `handle(msg, ctx)`.
- Fits *inside* a service when you have one piece of state with many concurrent mutators (e.g. the mount manager could be an Actor that receives `MountRequest`, `UnmountRequest`, `Subscribe` messages — and the current `Arc<RwLock<Option<MountManager>>>` plus public methods is a poorer version of that).
- Doesn't fit the daemon's *top-level* shape: we have many independent services, not one actor with structured state.

**Recommendation.** Adopt the `Service` trait at the daemon top level (P-1). Use Actor-style **only if** a sub-component grows enough state-and-concurrency complexity to need it — likely the MountManager (P-4), maybe the gateway cache, possibly the iroh protocol layer (but that's `zim-protocol`'s call). Don't pre-emptively retrofit actors elsewhere.

## 4. Prioritized improvements (target shape)

### P1 — Address now, before further daemon work
1. **`Service` trait + uniform spawn loop.** Collapses lifecycle bootstrap (pain points §P-1, §P-2, §P-7, §P-8). Clean break: replace `process::start_service` with a vec-of-services loop; delete `service_config::Config` if it folds into AppConfig (§P-3); rewrite `spawn_service` as "logging + start_service + wait."
2. **Single `AppConfig`** as source of truth. Delete `service_config::Config`. Server-specific configs are projections (§P-3). Loading paths consolidated through a `Layout` type (§P-5).
3. **Fix the sync worker shutdown leak.** Sync worker is a `Service` impl; spawned by the lifecycle layer; receives the shared `shutdown_rx`. Bug, not aesthetics. (§P-2)

### P2 — Address with the next feature pass
4. **Scoped event channel.** `tokio::sync::broadcast` + typed `Event` enum on AppState; SSE handler subscribes. Unlocks `zim watch`, hub live-progress, desktop UI reactivity. (§P-6)
5. **`Tasks` module.** Generalize sync_provider into a typed `TaskProducer + TaskWorker` split. Sync, indexing, retention, hub jobs all flow through it. (§P-7)
6. **Drop `Option<MountManager>` dance.** Initialize unconditionally (with feature flag); hold as `Arc<MountManager>`. Consider Actor-style if/when mount management grows beyond the current method surface. (§P-4)

### P3 — Address opportunistically (next time you're in the file)
7. **Handler-per-file with `views/` vs `actions/` split** under `http_server/api/v0/` and `http_server/gateway/`. Already mostly handler-per-file; the views/actions distinction is the one pack-pattern worth adding here.
8. **Drop `AsRef<Peer>` / `AsRef<Database>` on AppState.** Force handlers to declare what they actually need. (§P-9)
9. **Separate `init_logging` from `spawn_service`.** Embedded callers (Tauri/desktop, tests) call into a smaller surface. (§P-8)
10. **Typed `ShutdownToken`.** First-class shutdown trigger that's not coupled to the join-handle bag. (§P-10)

## 5. Out-of-scope notes (called out per task constraints)

- **Backwards compatibility / migration phases**: explicitly excluded by the clean-break broadcast and T-007's "Out of scope" list. The improvements above are *target-shape* statements. Implementation deletes the old surface; it does not deprecate it.
- **Adding daemon features**: out of scope. MCP endpoint, hub re-publisher, indexer, retention sweep are mentioned only as examples of services the post-P1 lifecycle would absorb without further bootstrap edits.
- **`zim-protocol` internals**: the peer's internal sync loop and protocol state machine are in a separate crate and a separate audit.

## 6. Suggested follow-up tasks

If orch wants to land this:
- **T-007a** (impl, `zim-peer` worker) — Introduce `Service` trait + uniform spawn loop; fix sync worker leak; consolidate config types. Touches `process/mod.rs`, `service_state.rs`, `service_config.rs`, `state.rs`, `sync_provider.rs`. P1 set above.
- **T-007b** (impl, `zim-peer` worker) — Events + Tasks module. Touches AppState plus emit sites across the daemon. P2 set.
- **T-007c** (docs, thing4) — `docs/architecture/` notes for the new lifecycle and event model.

Standing by. Will pivot to T-008 (per-file/folder publication) on orch's signal.
