# Sync Architecture Refactor — Research Report

**Date:** 2026-06-01
**Scope:** Reshape vault + sync surface so vaults open lazily, peers
negotiate ancestors directly, and side-effects become inspectable data
rather than imperative method calls.
**Reading order:** §0 → §1 → §2 → §3 → §4 → §5 → §6. §7 and §8 are
references.

## §0 Executive summary

zim already has most of the runtime parts an event-driven sync needs.
What's missing is shape:

1. **`Vault` is the working-copy primitive but the runtime doesn't open
   them lazily or cache them.** Today every `peer.mount(bucket_id)` call
   re-fetches the head and re-builds an `Fs<BlobsStore>`. `MountManager`
   caches FUSE mounts but not arbitrary vault opens. There's no LRU, no
   idle-close.
2. **Ancestor-finding is chatty.** `sync_bucket::find_common_ancestor`
   does one network round-trip per chain step
   (`sync_bucket.rs:288–346`). For a 50-step divergence that's 50
   sequential blob downloads — the peer holding the chain could answer
   in one message if we had a bisect protocol.
3. **The "side effects" hook exists but is imperative.** The
   `BidirectionalHandler::handle_message_side_effect` hook
   (`bidirectional.rs:88–99`) is the existing seam where post-response
   work happens. Today the body of each impl just calls
   `peer.dispatch(SyncJob::…)` directly, so the side effect is a verb,
   not a noun. Making it a noun (returning `Vec<Effect>`) unlocks
   testing, batching, ordering, and a single audit point.
4. **Three event channels exist, none unified.** `SyncEvent` (FUSE cache
   invalidation, `fuse/sync_events.rs`), `VaultEvent` (newly added,
   `vault/event.rs`), and `SyncJob` (the imperative job queue
   `sync_provider.rs`). They should collapse into one event taxonomy with
   typed subscribers.
5. **No actor surface for vaults.** `MountManager` stores
   `Arc<RwLock<Fs<BlobsStore>>>` — direct shared mutability. Every
   reader/writer takes the same lock. A per-vault actor with a typed
   mailbox would (a) serialize fs ops cleanly, (b) make idle-close
   straightforward (drop the actor), and (c) be the natural home for
   apply_chain.

**The proposal** (§4) is a five-layer split:

```
            ┌──────────────────────────────────────────┐
   peers →  │ Wire protocol (zim-protocol)             │
            │   HeadRequest / AncestorProbe / etc.     │
            └──────────────────────────────────────────┘
                              ↓ Effect
            ┌──────────────────────────────────────────┐
            │ SyncCoordinator (zim-peer)               │
            │   vault registry, peer hint table        │
            └──────────────────────────────────────────┘
                              ↓ command via mailbox
            ┌──────────────────────────────────────────┐
            │ VaultActor (per open vault)              │
            │   owns Vault, serializes fs ops          │
            └──────────────────────────────────────────┘
                              ↓
            ┌──────────────────────────────────────────┐
            │ Vault (zim-core)                         │
            │   Fs + Log + emit VaultEvent             │
            └──────────────────────────────────────────┘
                              ↓
            ┌──────────────────────────────────────────┐
            │ VaultLog (per-vault, no open required)   │
            │   probe(), exponential_sample(), …       │
            └──────────────────────────────────────────┘
```

The jig actor model (§2) supplies the mailbox shape. The existing
`BidirectionalHandler` pattern supplies the responder/initiator
symmetry. The `Effect` enum (§4.4) is the new thing. Migration §6 is
ordered to keep zim-core green throughout; zim-protocol's existing
breakage is dealt with in two phases.

---

## §1 Current state

### §1.1 Crates and where the relevant code lives

```
zim-core/
  src/blobs/object_store/actor.rs    (981 LOC) — ObjectStoreActor: tokio task
  src/vault/vault.rs                  (~400 LOC) — Vault (post-Pass-1)
  src/vault/chain.rs                  (~175 LOC) — find_common_ancestor (two-pointer)
  src/vault/event.rs                  (NEW)    — VaultEvent
  src/vault/log/                              — VaultLog trait + Sqlite/Memory impls
zim-protocol/
  src/peer/peer_inner.rs              (478 LOC) — Peer<L>
  src/peer/sync/sync_bucket.rs        (591 LOC) — execute(), find_common_ancestor()
  src/peer/sync/ping_peer.rs                  — ping job
  src/peer/sync/download_pins.rs              — pin download job
  src/peer/protocol/bidirectional.rs (266 LOC) — BidirectionalHandler trait
  src/peer/protocol/messages/ping.rs (~300 LOC) — Ping impl of the handler
zim-peer/
  src/sync_provider.rs                (282 LOC) — QueuedSyncProvider + run_worker
  src/fuse/mount_manager.rs           (637 LOC) — MountManager, LiveMount
  src/fuse/sync_events.rs             (17 LOC)  — SyncEvent enum
  src/fuse/fuse_fs.rs                          — FUSE listener
  src/backup_sync.rs                  (185 LOC) — poll-based backup service
```

### §1.2 The bidirectional handler pattern (the kernel)

`BidirectionalHandler` is the most actor-shaped thing in the codebase.
For each message type:

- `wrap_request(req) → Message` — embeds into the network enum.
- `handle_message(peer, sender, req) → Reply` — pure: input + state →
  output.
- `handle_reply(peer, recipient, reply) → Result<()>` — initiator action.
- `handle_message_side_effect(peer, sender, req, reply) → Result<()>` —
  fire-and-forget background work after the reply is on the wire.

The trait is at `crates/zim-protocol/src/peer/protocol/bidirectional.rs:26`.
The provided `_handle_message` method at `:128–177` runs
`handle_message → serialize+send → handle_message_side_effect`, so the
side effect happens *after* the network reply is sent. That's exactly
the ordering you want for "ack first, do background work second."

**This is most of what we want.** The gaps:

1. `handle_message_side_effect` is imperative — it calls `peer.dispatch`
   or peer methods directly. You can't write a unit test that says
   "after receiving this ping, exactly this sync job should be enqueued."
   You'd have to mock `Peer`.
2. There's only one side-effect hook per handler. No symmetric hook on
   the initiator side (the initiator does its side-effects inline in
   `handle_reply`).
3. The handler only models bidirectional request/reply. There's no
   one-shot push message ("HeadAdvanced") in the trait.

### §1.3 Sync flow today, end-to-end

Trigger: peer A's daemon does any of {periodic ping tick fires, FUSE
write causes save, API write causes save}. Let's trace the periodic
ping path because it exercises the most:

1. `run_worker` (sync_provider.rs:147–207) fires `ping_interval.tick()`.
2. `schedule_periodic_pings` (sync_provider.rs:216) iterates
   `list_syncable_buckets` and calls `peer.ping(bucket_id)`
   (peer_inner.rs:148).
3. `peer.ping` loads the current head manifest (peer_inner.rs:160),
   reads `manifest.shares()`, and dispatches a `PingPeerJob` for each
   peer in shares (peer_inner.rs:179).
4. Dispatched jobs go through `SyncProvider::execute`. In
   `QueuedSyncProvider` that means `tx.try_send(job)` to the flume
   channel (sync_provider.rs:69).
5. `run_worker` consumes the job, sees `SyncJob::PingPeer`, spawns a
   detached task that calls `ping_peer::execute` under a 10-permit
   semaphore (sync_provider.rs:165–173).
6. `ping_peer::execute` calls `Ping::send` (BidirectionalHandler) which
   opens an iroh bidirectional stream to the remote peer
   (bidirectional.rs:193).
7. Remote peer's ALPN accept handler reads the `PingMessage`,
   dispatches to `Ping::_handle_message`
   (bidirectional.rs:128) which calls `Ping::handle_message`
   (ping.rs:103–130). That compares heights and returns
   `Ahead/Behind/InSync/NotFound`.
8. Remote sends reply, then runs `handle_message_side_effect`
   (ping.rs:136). If status is `Behind`, that side effect calls
   `peer.dispatch(SyncJob::SyncBucket(…))` (ping.rs:181) — enqueuing
   *its own* sync against the initiator.
9. Initiator receives reply, runs `handle_reply` (ping.rs:245). If
   status is `Ahead`, it dispatches a sync against the responder
   (ping.rs:281, similar shape).
10. Eventually a `SyncBucketJob` reaches `sync_bucket::execute`
    (sync_bucket.rs:48). This:
    - Checks `exists`
    - Calls `find_common_ancestor` which does
      **download-manifest → check log → walk previous** in a loop
      (sync_bucket.rs:288–346, one network round-trip per step)
    - Calls `download_manifest_chain` from target → ancestor
      (sync_bucket.rs:176, again one round-trip per step)
    - Verifies authorship
    - Calls `apply_manifest_chain` (sync_bucket.rs:352) which
      appends each manifest to the log and dispatches
      `SyncJob::DownloadPins` for each.
11. Each `DownloadPinsJob` runs in the worker, fetching each pinned
    hash from peers.
12. After sync, `MountManager::on_bucket_synced` is called externally,
    which (if a FUSE mount exists for this bucket) either merges or
    reloads, then broadcasts `SyncEvent::MountInvalidated` to FUSE
    cache listeners (mount_manager.rs:146–238).

**Observations:**

- The flow already has well-defined seams: protocol-handler →
  side-effect → dispatch → worker → sync job → log update → FUSE
  refresh. The seams are imperative method calls; nothing inspects what
  flows through.
- `find_common_ancestor` in (10) and the chain walk both do one
  network round-trip per step. This is the obvious slow path.
- `MountManager::on_bucket_synced` is called from… where? Searching the
  codebase, it's called from sync paths in zim-peer; the wiring is
  manual. There's no general "vault state advanced" pub/sub.

### §1.4 Three event channels, none unified

| Channel | Type | Producer | Consumer | Lives in |
|---|---|---|---|---|
| `broadcast<SyncEvent>` | `BucketUpdated`, `MountInvalidated` | `MountManager::on_bucket_synced` | FUSE `spawn_sync_listener` | zim-peer |
| `broadcast<VaultEvent>` | `Saved`, `Synced` | `Vault::save`, `Vault::sync_with` | none yet (opt-in via `with_events`) | zim-core |
| `flume<SyncJob>` | `SyncBucket`, `DownloadPins`, `PingPeer` | protocol handlers, periodic scheduler, API | `run_worker` background task | zim-peer (queue) / zim-protocol (executor) |

`SyncEvent` is intrinsically FUSE-shaped. `VaultEvent` is the right
shape but has no subscribers. `SyncJob` is the imperative queue. These
should collapse into one `Effect` taxonomy where some variants are
"emit to subscribers" and others are "execute background work."

### §1.5 What's broken in zim-protocol

~35 compile errors, all surfacing as side effects of upstream type
changes the protocol crate hasn't been swept for:

| Bucket | Count | Files | Fix shape |
|---|---|---|---|
| `PublicKey` no `Display` | 7 | peer_inner.rs, ping.rs, sync_bucket.rs | mechanical: `.to_hex()` |
| `&Hash` not `Into<Hash>` | 7 | content_store.rs (via `get_cbor(&hash())`) | mechanical: pass `*hash` or call site change |
| `HashSet<&String>` vs `&PublicKey` | 5 | sync_bucket.rs | requires deciding share-key type |
| Missing generic | 2 | ping.rs, sync_bucket.rs | mechanical: `get_cbor::<T, _>` |
| Misc type mismatches | 14 | peer_inner.rs, sync_bucket.rs | mix of mechanical + structural |

None of this is a control-flow bug. It's a long-deferred sweep after
`PublicKey` and `Hash` semantics changed in zim-core. Importantly:
**the breakage is concentrated in exactly the code we want to
restructure** — `sync_bucket.rs`, `peer_inner.rs`, `ping.rs`. So
the rehab and the refactor share a workspace.

---

## §2 The jig actor model (the inspiration)

Reference: `krondor-corp/jig/crates/jig-cli/src/daemon/actors/actor.rs`

### §2.1 Anatomy

```rust
pub trait Actor: Default + Send + Sync + 'static {
    type Request: Send + 'static;
    type Response: Send + 'static;
    const NAME: &'static str;
    const QUEUE_SIZE: usize;
    fn handle(&self, req: Self::Request) -> Self::Response;
}

pub struct ActorHandle<A: Actor> {
    tx: flume::Sender<A::Request>,
    rx: flume::Receiver<A::Response>,
    inner: Arc<A>,                    // shared with bg thread
    pending: Arc<AtomicBool>,         // "one inflight at a time"
    _handle: std::thread::JoinHandle<()>,
}
```

- One actor = one struct that implements `Actor`. The struct IS the
  state. Interior mutability (`Mutex`, `AtomicBool`) for fields that
  mutate, because `handle(&self)` takes a shared reference (background
  thread + main thread can both read).
- `ActorHandle` wraps the channels, spawns a **`std::thread`** (not a
  tokio task) named `A::NAME`, and exposes:
  - `send(req) → bool` — non-blocking, returns `false` if a request is
    already pending. Sets `pending=true` on success.
  - `drain() → Vec<Response>` — non-blocking, collects whatever the
    actor has produced. Used in main-loop tick.
  - `actor() → &A` — read actor state directly (for atomics / locked
    fields).
- Bounded flume channel sized by `A::QUEUE_SIZE`.

### §2.2 What it does well

1. **The actor IS the state.** No separate state struct, no
   builder/inject pattern. You define `MyActor { foo: Mutex<X> }` and
   that's the whole thing.
2. **One outstanding request at a time.** The `pending` flag enforces
   it. This is opinionated; it suits long-running actors that shouldn't
   pile up work (e.g. an indexer, a renderer). For sync we'd relax this.
3. **Shared state access via `Arc<A>`.** The main thread can call
   `handle.actor().some_atomic.load(Relaxed)` without going through the
   mailbox. This is the escape hatch that makes the pattern usable for
   "I just need the current value" without round-tripping.
4. **`drain()` is poll-shaped.** No async await needed — the main loop
   pulls responses on its tick. This matches a GUI/game-loop style
   runtime.
5. **Per-actor named thread.** Easier to attribute CPU/profile output.

### §2.3 What it doesn't try to solve

- **Supervision / restart.** If `handle()` panics, the thread dies and
  the handle holds a closed channel. No supervisor restarts it.
- **Multi-message types.** One `Request` enum per actor; if you want
  different shapes you wrap them in an enum yourself.
- **Async I/O inside `handle()`.** The function is `fn`, not `async fn`.
  Jig uses sync I/O in actors. zim is tokio-async — we'd want
  `async fn handle`.
- **Backpressure on overflow.** `send` returns `false` if pending; doesn't
  block, doesn't await, doesn't queue. Caller decides.

### §2.4 Lessons for zim

Adopt:

- **Actor = struct + trait.** Each actor type has a typed mailbox and
  owned state.
- **Handle exposes mailbox + state accessor.** `actor()` for reads,
  `send/call` for writes.
- **One handle per "thing".** Per-vault actor, per-coordinator actor.

Adapt:

- **Tokio task, not std::thread.** zim is async top-to-bottom.
- **`async fn handle`.** Async I/O is required for blob fetches.
- **`call(req).await → Response`.** Request/response with a one-shot
  reply channel, instead of fire-and-forget + drain. Jig's pattern is
  great for "main loop polls"; ours wants direct call-and-await for
  caller convenience. We can still have `send_fire_and_forget` for
  cases that don't need a reply.
- **No "single inflight" restriction.** Sync work is parallelizable
  across vaults; we want a `mpsc` queue with reasonable depth, not a
  pending-flag lock.

---

## §3 The gap (in detail)

### §3.1 No vault registry above MountManager

`MountManager.mounts: RwLock<HashMap<Uuid, LiveMount>>` is a registry
but only for FUSE-mounted vaults. If an HTTP API handler wants to
`peer.mount(bucket_id)` to read a file, it gets a fresh `Fs::load`
every time. Three implications:

- Repeat load cost: each call re-parses the manifest envelope, re-loads
  the metadata pack, re-walks shares. Manifest blob is cached in blobs
  but the Fs construction is not.
- Memory: there's no LRU. If a daemon serves 1000 buckets and FUSE
  mounts 5, the other 995 are open at most once and then dropped.
- Coordination: if two callers both `mount(bucket_id)` and both write,
  they end up with separate `Arc<Mutex<FsInner>>` instances and the
  second save races.

A `VaultRegistry: HashMap<Uuid, VaultActor>` solves all three. Idle
actors auto-close after, say, 5 minutes of no traffic.

### §3.2 Side effects are imperative

The `handle_message_side_effect` body in `Ping` is ~80 lines of
imperative `peer.dispatch(SyncJob::SyncBucket {…})` calls
(ping.rs:144–238). Three observations:

- **You can't unit-test it.** To test "Ping side effect when Behind",
  you'd construct a fake `Peer<L>`, run the side effect, then assert
  on… what? The SyncProvider mock's recorded calls. Painful.
- **You can't batch.** Two consecutive Pings against different peers
  for the same bucket both dispatch separate `SyncBucketJob`s with
  potentially-overlapping peer lists. A coordinator could merge.
- **You can't audit.** The flow of side effects through the system is
  invisible. Tracing helps but has its own cost.

If `handle_message_side_effect` returned `Vec<Effect>`:

```rust
enum Effect {
    Sync { bucket_id: Uuid, target: SyncTarget },
    DownloadPins { pins_link: Link, peer_ids: Vec<PublicKey> },
    Ping { bucket_id: Uuid, peer_id: PublicKey },
    EmitVaultEvent(VaultEvent),
    EmitPeerEvent(PeerEvent),
}
```

Then `_handle_message` (the provided method in the trait) becomes:

```rust
let effects = Self::handle_message_side_effect(peer, sender, &req, &reply).await?;
for effect in effects {
    coordinator.apply(effect).await;
}
```

The handler is pure (state + input → effects). The coordinator decides
how to schedule. Tests assert on the returned `Vec<Effect>`. Production
behavior unchanged.

### §3.3 No probe / bisect

`PingMessage` carries `(bucket_id, link, height)` — the initiator's
current head. The responder can only answer with their head. That's a
1-pair-per-round-trip exchange.

For ancestor finding we want either:

- **AncestorProbe** — initiator sends a height-keyed sample
  `[(h, link), (h-1, link), (h-2, link), (h-4, link), …]`. Responder
  returns the deepest height where it shares a link.
- **Or extend Ping** with a `sample: Vec<(Height, Link)>` field and have
  the reply include a `common_ancestor: Option<(Link, Height)>`.

I lean toward a separate `AncestorProbe` message because Ping is
already overloaded (it's the periodic "are we in sync?" *and* the
height comparator). Adding a sample field bloats the wire shape for
the common-case ping that already-knows-they-agree.

### §3.4 No push (no gossip / no head broadcast)

When peer A saves, peer B finds out via periodic ping (5 min interval).
That's a hard latency floor. Two ways to fix:

1. **HeadAdvanced unicast.** When peer A saves, dispatch `Ping` to every
   peer in shares with the new height. Same protocol, same shape, just
   a "I changed" trigger. Cheap. Latency = number-of-shares ×
   one-shot-message.
2. **Gossip topic per vault.** Subscribe to iroh-gossip topic
   `vault:{uuid}` when opening a vault. Save broadcasts to topic; all
   subscribers receive without a unicast fan-out.

The user said "OK to add events for peers to find out their common
ancestor together" — that's compatible with (1). I'd start there;
gossip is a deeper change and iroh-gossip is not currently a dep.

### §3.5 No vault-actor surface

`MountManager`'s `Arc<RwLock<Fs<BlobsStore>>>` is functional but loose.
Six places in the codebase take `mount.read().await` and another four
take `mount.write().await`. Every reader sees the latest state, but
write coordination is per-caller. A VaultActor with a typed mailbox
makes the contract explicit:

```rust
enum VaultCommand {
    Add { path: AbsPath, data: Bytes, reply: oneshot::Sender<Result<()>> },
    Mkdir { path: AbsPath, parents: bool, reply: oneshot::Sender<Result<()>> },
    // ...
    ApplyChain { target_link: Link, ancestor: Option<Link>, peer_ids: Vec<PublicKey>, reply: oneshot::Sender<Result<()>> },
    Snapshot { reply: oneshot::Sender<FsInner> },  // for read-only callers
    Close { reply: oneshot::Sender<()> },
}
```

Every fs mutation is a command. Reads that don't need fresh data can
use the snapshot. The actor serializes ops within itself.

This isn't strictly necessary if `Vault::fs` retains its
`Arc<Mutex<FsInner>>` shape — the lock already serializes — but the
actor gives us: explicit command types, single audit point, idle-close
hook, and a place to push `Effect`s.

---

## §4 Proposed architecture

### §4.1 Five-layer split (recap)

```
Wire protocol (zim-protocol):
    HeadRequest, AncestorProbe, HeadAdvanced
       ↓ returns Vec<Effect>
SyncCoordinator (zim-peer):
    vault registry, peer hint table, event bus
       ↓ command via VaultActor mailbox
VaultActor (zim-peer or zim-core):
    one per open vault, owns Vault, serializes fs ops
       ↓
Vault (zim-core):
    Fs + Log + BlobsProvider + emit VaultEvent
       ↓
VaultLog (zim-core):
    probe(), exponential_sample() — answers most peer queries
    without opening the vault
```

### §4.2 Vault: working-copy primitive

Stays as we have it (post-Pass-1) with one signature change. Drop the
self-managed sync_with — that's the coordinator's job:

```rust
pub struct Vault<L: VaultLog> {
    id: Uuid,
    blobs: BlobsProvider,
    log: L,
    endpoint: Endpoint,
    fs: Fs<BlobsProvider>,
    events: Option<broadcast::Sender<VaultEvent>>,
}

impl<L: VaultLog> Vault<L> {
    pub async fn init(...) -> Self;
    pub async fn open(...) -> Self;
    pub fn with_events(self, tx) -> Self;

    // Delegators (unchanged).
    pub async fn add(...);
    pub async fn mkdir(...);
    // ...

    pub async fn save(&mut self) -> Result<Link>;

    // The new primitive: apply a remote chain to log + working copy.
    // Blobs are assumed reachable via `peer_ids`.
    pub async fn apply_chain(
        &mut self,
        target_link: Link,
        target_height: u64,
        ancestor: Option<Link>,
        peer_ids: Vec<PublicKey>,
    ) -> Result<Link>;

    pub async fn merge_with(&mut self, incoming_link: &Link) -> Result<(MergeResult, Link)>;
}
```

`apply_chain` replaces today's `sync_with`. Same body but:

- Takes `ancestor` as an argument (the coordinator found it via probe).
- Doesn't do its own `find_common_ancestor` walk.
- Still downloads manifests + verifies + appends to log + downloads pins.
- After apply, calls `merge_with(target_link)` if the working copy was
  open at start. Otherwise just emits `Synced`.

### §4.3 VaultLog: probe + chain sampling

Add three methods to the `VaultLog` trait:

```rust
#[async_trait]
pub trait VaultLog {
    // ... existing ...

    /// Given a peer's height-keyed sample of their chain, return the
    /// deepest one we share. O(sample.len()) lookups, each indexed.
    async fn probe(
        &self,
        id: Uuid,
        sample: &[(u64, Link)],
    ) -> Result<Option<(Link, u64)>, VaultLogError<Self::Error>>;

    /// Build an exponentially-spaced sample of our chain ending at
    /// the head, suitable for sending in an AncestorProbe.
    /// Returns [(h, link), (h-1, link), (h-2, link), (h-4, link), …]
    /// down to height 0.
    async fn exponential_sample(
        &self,
        id: Uuid,
    ) -> Result<Vec<(u64, Link)>, VaultLogError<Self::Error>>;

    /// Iterator over (height, link) pairs in descending height order.
    /// Default implementation walks one fetch per height; sqlite impl
    /// overrides with a single range query.
    async fn chain(
        &self,
        id: Uuid,
    ) -> Result<Vec<(u64, Link)>, VaultLogError<Self::Error>>;
}
```

`exponential_sample` is the bisect generator. Round-trip
characteristics:

| Divergence depth d | Sample size | Probes needed |
|---|---|---|
| 0 (in sync) | 1 | 1 |
| 1–10 | log₂(d)+2 ≈ 5 | 1 |
| 100 | ≈9 | 1 |
| 10,000 | ≈16 | 1 |

So one round-trip handles everything up to 16k version divergence.
Beyond that, the responder reports its deepest match and the initiator
sends a refined sample around that height.

### §4.4 Effect / Outbox: side effects become data

```rust
#[derive(Debug, Clone)]
pub enum Effect {
    // ── Local commands ──
    /// Open a vault if not already; queue a command against it.
    VaultCommand { id: Uuid, cmd: VaultCommand },

    /// Apply a remote chain end-to-end (the coordinator turns this
    /// into: probe → vault.apply_chain).
    ApplyRemoteChain {
        id: Uuid,
        target_link: Link,
        target_height: u64,
        peer_ids: Vec<PublicKey>,
    },

    /// Download specific blob hashes (e.g. pins).
    DownloadBlobs {
        hashes: Vec<Hash>,
        peer_ids: Vec<PublicKey>,
    },

    // ── Peer messages ──
    SendPing { peer_id: PublicKey, bucket_id: Uuid },
    SendAncestorProbe { peer_id: PublicKey, bucket_id: Uuid, sample: Vec<(u64, Link)> },
    SendHeadAdvanced { peer_id: PublicKey, bucket_id: Uuid, head: Link, height: u64 },

    // ── Event broadcasts ──
    EmitVaultEvent(VaultEvent),
    EmitPeerEvent(PeerEvent),    // new — for FUSE/UI subscribers
}
```

The `BidirectionalHandler` trait changes:

```rust
pub trait BidirectionalHandler: Sized {
    type Message: ...;
    type Reply: ...;

    fn wrap_request(req: Self::Message) -> Message;

    async fn handle_message<L>(peer: &Peer<L>, sender: &PublicKey, msg: &Self::Message) -> Self::Reply;

    /// Returns effects to apply after the reply is sent.
    async fn handle_message_effects<L>(
        peer: &Peer<L>,
        sender: &PublicKey,
        msg: &Self::Message,
        reply: &Self::Reply,
    ) -> Vec<Effect> { vec![] }

    /// Returns effects to apply after the reply is received.
    async fn handle_reply_effects<L>(
        peer: &Peer<L>,
        recipient: &PublicKey,
        reply: &Self::Reply,
    ) -> Vec<Effect> { vec![] }
}
```

And a runtime adapter:

```rust
pub async fn apply_effects(coordinator: &SyncCoordinator, effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::VaultCommand { id, cmd } => coordinator.send_to_vault(id, cmd).await,
            Effect::ApplyRemoteChain { id, target_link, target_height, peer_ids } =>
                coordinator.schedule_apply_chain(id, target_link, target_height, peer_ids).await,
            Effect::DownloadBlobs { hashes, peer_ids } => coordinator.download_many(hashes, peer_ids).await,
            Effect::SendPing { peer_id, bucket_id } => coordinator.send_ping(peer_id, bucket_id).await,
            Effect::SendAncestorProbe { peer_id, bucket_id, sample } =>
                coordinator.send_probe(peer_id, bucket_id, sample).await,
            Effect::SendHeadAdvanced { peer_id, bucket_id, head, height } =>
                coordinator.send_head_advanced(peer_id, bucket_id, head, height).await,
            Effect::EmitVaultEvent(e) => coordinator.vault_events.send(e),
            Effect::EmitPeerEvent(e) => coordinator.peer_events.send(e),
        }
    }
}
```

Properties:

- **Pure handlers.** `handle_message_effects` and `handle_reply_effects`
  are easy to unit-test. Given input + state, assert on the returned
  Vec.
- **Single dispatch point.** All side effects funnel through
  `apply_effects`. Tracing, metrics, rate-limiting all attach there.
- **Composable.** A handler can return multiple effects, including
  ones that span layers (a peer message effect *and* a local event
  emission).
- **The SyncJob queue goes away.** What was `SyncJob::SyncBucket` is now
  `Effect::ApplyRemoteChain` returned from `handle_reply_effects`.
  `QueuedSyncProvider`'s job becomes "execute Effects" — same
  shape, different vocabulary.

### §4.5 VaultActor: per-vault mailbox, lazy-open, idle-close

```rust
pub struct VaultActor<L: VaultLog> {
    inner: Vault<L>,
    cmd_rx: mpsc::Receiver<VaultCommand>,
    last_activity: Instant,
}

pub enum VaultCommand {
    Add { path: AbsPath, data: Bytes, reply: oneshot::Sender<Result<()>> },
    Mkdir { path: AbsPath, parents: bool, reply: oneshot::Sender<Result<()>> },
    Rm { path: AbsPath, reply: oneshot::Sender<Result<()>> },
    Mv { from: AbsPath, to: AbsPath, reply: oneshot::Sender<Result<()>> },
    Save { reply: oneshot::Sender<Result<Link>> },
    ApplyChain {
        target_link: Link, target_height: u64,
        ancestor: Option<Link>, peer_ids: Vec<PublicKey>,
        reply: oneshot::Sender<Result<Link>>,
    },
    Snapshot { reply: oneshot::Sender<FsInner> },
    Cat { path: AbsPath, reply: oneshot::Sender<Result<Vec<u8>>> },
    Ls { path: AbsPath, reply: oneshot::Sender<Result<BTreeMap<...>>> },
    // ... one variant per Vault method that callers need ...
}

pub struct VaultActorHandle {
    cmd_tx: mpsc::Sender<VaultCommand>,
    join: JoinHandle<()>,
}

impl<L: VaultLog> VaultActor<L> {
    pub fn spawn(vault: Vault<L>, queue: usize) -> VaultActorHandle {
        let (cmd_tx, cmd_rx) = mpsc::channel(queue);
        let mut actor = Self { inner: vault, cmd_rx, last_activity: Instant::now() };
        let join = tokio::spawn(async move { actor.run().await });
        VaultActorHandle { cmd_tx, join }
    }

    async fn run(&mut self) {
        let mut idle = tokio::time::interval(Duration::from_secs(60));
        loop {
            tokio::select! {
                Some(cmd) = self.cmd_rx.recv() => {
                    self.last_activity = Instant::now();
                    self.dispatch(cmd).await;
                }
                _ = idle.tick() => {
                    if self.last_activity.elapsed() > Duration::from_secs(300) {
                        tracing::info!("VaultActor {} idle, closing", self.inner.id());
                        break;
                    }
                }
                else => break,
            }
        }
    }
}
```

Per-vault mailbox, idle-close on 5min, all fs ops sequential within an
actor. Callers `await` a reply; lock contention disappears (the actor
owns the Vault outright).

### §4.6 SyncCoordinator

```rust
pub struct SyncCoordinator<L: VaultLog> {
    vaults: RwLock<HashMap<Uuid, VaultActorHandle>>,
    blobs: BlobsProvider,
    log: L,                          // log factory; per-vault loaded on open
    endpoint: Endpoint,
    secret: PrivateKey,
    peer_hints: RwLock<HashMap<Uuid, Vec<PublicKey>>>,  // recent peers per vault
    vault_events: broadcast::Sender<VaultEvent>,
    peer_events: broadcast::Sender<PeerEvent>,
}

impl<L: VaultLog> SyncCoordinator<L> {
    /// Get or spawn the VaultActor for `id`.
    async fn open_vault(&self, id: Uuid) -> Result<VaultActorHandle> { ... }

    /// Respond to a HeadRequest. Log-only, no vault open.
    pub async fn handle_head_request(&self, id: Uuid) -> Option<(Link, u64)> {
        self.log.head(id, None).await.ok()
    }

    /// Respond to an AncestorProbe. Log-only, no vault open.
    pub async fn handle_ancestor_probe(
        &self, id: Uuid, sample: &[(u64, Link)],
    ) -> Option<(Link, u64)> {
        self.log.probe(id, sample).await.ok().flatten()
    }

    /// Apply Effects returned by handlers.
    pub async fn apply_effects(&self, effects: Vec<Effect>) { ... }

    /// End-to-end pull: probe → apply_chain.
    pub async fn pull_from(&self, id: Uuid, peer: PublicKey) -> Result<()> {
        let remote = peer_message::head_request(peer, id).await?;
        let Some((remote_link, remote_height)) = remote else { return Ok(()) };
        if self.log.head(id, None).await?.1 >= remote_height { return Ok(()) }

        let sample = self.log.exponential_sample(id).await?;
        let ancestor = peer_message::ancestor_probe(peer, id, sample).await?;

        let actor = self.open_vault(id).await?;
        let (tx, rx) = oneshot::channel();
        actor.cmd_tx.send(VaultCommand::ApplyChain {
            target_link: remote_link, target_height: remote_height,
            ancestor: ancestor.map(|(l, _)| l),
            peer_ids: vec![peer],
            reply: tx,
        }).await?;
        rx.await??;
        Ok(())
    }
}
```

This is the only place that knows about peers, channels, and vault
opening. Everything else is wired through Effects.

### §4.7 Wire protocol additions

Three new messages, all implementing `BidirectionalHandler`:

```rust
// Replaces today's Ping's height-comparison role.
pub struct HeadRequest { pub bucket_id: Uuid }
pub struct HeadReply { pub bucket_id: Uuid, pub head: Option<(Link, u64)> }

pub struct AncestorProbe {
    pub bucket_id: Uuid,
    pub sample: Vec<(u64, Link)>,
}
pub struct AncestorReply {
    pub bucket_id: Uuid,
    pub highest: Option<(Link, u64)>,
}

// Unidirectional push (no reply needed).
pub struct HeadAdvanced {
    pub bucket_id: Uuid,
    pub head: Link,
    pub height: u64,
    pub from: PublicKey,        // originator hint
}
```

Today's `Ping` stays as the periodic heartbeat (its real job:
keep-alive + "do we still agree?"). Its `handle_reply_effects` returns
`Effect::ApplyRemoteChain` when out-of-sync, which the coordinator
turns into probe → apply.

`HeadAdvanced` is fire-and-forget unicast. After a `Vault::save`, the
coordinator iterates `manifest.shares()` and sends `HeadAdvanced` to
each. Recipients respond by enqueuing `Effect::ApplyRemoteChain`.

### §4.8 BlobsProvider stays content-addressed, takes peer hints

You said "it needs a list of peers at least" — confirmed. `apply_chain`
takes `peer_ids: Vec<PublicKey>`. `BlobsProvider::download_hash`
already has this shape (`peer.blobs().download_hash(hash, peers, endpoint)`
in sync_bucket.rs:201). No change there; just propagate the parameter
through the new APIs.

---

## §5 Data structures and adoptions

### §5.1 Exponential bisect sampling

Standard git-style. From height `h` build:

```
[h, h-1, h-2, h-4, h-8, h-16, …, h-2^k, …, 0]
```

while terms remain `>= 0` and deduplicate. For `h = 100`:

```
[100, 99, 98, 96, 92, 84, 68, 36, 0]
```

9 entries, covers full range. Pre-stored in the log as a single SQL
query over the heights index. Implementation goes in
`VaultLog::exponential_sample` with a sensible default (walk
`height(id)` and pick the right rows).

For the responder, `probe` is a `WHERE link IN (...) ORDER BY height DESC LIMIT 1`
on the indexed log. ~microseconds.

### §5.2 LRU-ish vault registry

`SyncCoordinator.vaults: RwLock<HashMap<Uuid, VaultActorHandle>>`.

Idle-close drops the entry when the actor exits. Two policies:

- **Idle timeout** (preferred): VaultActor exits after N minutes of no
  commands. Simple. The handle's `join` is awaited by a janitor that
  removes the map entry.
- **LRU cap**: at most N open vaults; opening another evicts the
  least-recently-used. Add only if the idle timeout doesn't keep
  memory bounded.

A vault being "open" means `Fs::load` cost (manifest fetch + metadata
pack decode + share recovery + root dir-body decrypt). Re-open cost on
miss is ~the same per call as today, so even with eviction the worst
case is what we have now.

### §5.3 Outbox: `Effect` enum

Already sketched in §4.4. The shape borrows from event-sourced systems
(Akka, CQRS) but stays small. Key invariants:

- `Effect` is `Send + 'static`. No references, no lifetimes.
- `Effect` is `Clone + Debug`. Useful for `tracing` and tests.
- Variants are flat — no nested `Box<dyn …>`. Keep it boring.
- Each variant maps to exactly one `SyncCoordinator` method.

### §5.4 Optional: iroh-gossip for HeadAdvanced fanout

For vaults with many holders (>10), unicast `HeadAdvanced` becomes
O(N) per save. iroh-gossip would let you publish to a topic keyed by
vault UUID. Holders subscribe on open, unsubscribe on close.

Cost: new dep. iroh-gossip is part of the iroh ecosystem so it'll
align well, but adds a ~few kLOC compile burden. **Defer** until N
gets high enough to matter. Implementation hint: design the
`HeadAdvanced` handler so it's identical whether the message came in
via gossip topic or direct unicast — the message body is the same.

### §5.5 Optional: skiplist on heights for chain walks

If `chain(id)` becomes a hot path, indexing the height column gives
us O(log N) lookup but full chain iteration is still O(N). A skiplist
or sparse index (only every k-th height materialized with a
"jump-back-k" pointer) could let `find_common_ancestor` skip in
constant amortized time per step. Mention for completeness — only
worth doing if the chain length becomes a real bottleneck. With the
probe protocol in place, ancestor finding rarely walks at all.

---

## §6 Migration sequence

Designed to keep zim-core green at every step. zim-protocol stays
broken until phase 3 because its breakage overlaps with the parts we're
restructuring; we rehab and refactor it in the same sweep.

### Phase 1 — zim-core foundations (small, low-risk)

1. **`VaultLog::probe` + `VaultLog::exponential_sample`**
   - Add trait methods with defaults that fall back to walking.
   - Implement in `SqliteVaultLog` and `MemoryVaultLog`.
   - Unit tests for both.

2. **Split `Vault::sync_with` → `Vault::apply_chain`**
   - Take `ancestor: Option<Link>` instead of finding it.
   - Same body otherwise.
   - Update the `vault::merge` free function path if needed.

3. **`Effect` enum + `EffectRuntime` trait skeleton**
   - Just the data types in `crates/zim-core/src/vault/effect.rs`.
   - No runtime yet; just the enum + From conversions.

4. **VaultActor + VaultActorHandle in zim-core**
   - Behind `Vault::spawn_actor(self, queue_size) → VaultActorHandle`.
   - Tests: spawn an actor, send Add, Save, ApplyChain, Snapshot,
     verify replies.

Risk: low. zim-core stays green. New code, no existing call sites to
update.

### Phase 2 — zim-protocol rehab + handler effect-ification

5. **Fix the 35 compile errors mechanically.**
   - `.to_hex()` for Display sites.
   - `*hash` for `&Hash` → `Hash` sites.
   - Generic args on `get_cbor::<T, _>`.
   - Decide on the `Manifest::shares()` key type
     (`HashMap<PublicKey, Share>` vs the current `HashMap<String, Share>`
     hex-keyed). Recommend converting to `PublicKey` keys; that
     eliminates 5 of the comparison errors.

6. **Add `HeadRequest`, `AncestorProbe`, `HeadAdvanced` messages.**
   - Three new files in `crates/zim-protocol/src/peer/protocol/messages/`.
   - Each impls `BidirectionalHandler` (or for `HeadAdvanced`, a
     one-shot push variant).
   - Effects returned, not dispatched.

7. **Convert `Ping::handle_message_side_effect` to return `Vec<Effect>`.**
   - Add the new effect-returning trait methods alongside the old
     side-effect ones; mark old as deprecated.
   - One handler at a time. Ping first.

Risk: medium. zim-protocol's rehab touches a lot of files; some of the
PublicKey/String decisions will surface their own questions.

### Phase 3 — zim-peer coordinator + wire-up

8. **Implement `SyncCoordinator`.**
   - Replaces `QueuedSyncProvider`-as-orchestrator (the worker stays as
     the Effect runtime; the dispatcher logic moves to the coordinator).
   - Vault registry with idle timeout.
   - `apply_effects` loop.
   - Wire `MountManager` to subscribe to `vault_events` instead of being
     called from sync paths.

9. **HTTP API handlers go through `SyncCoordinator.open_vault`.**
   - Replace direct `peer.mount(bucket_id)` calls.
   - Same behavior, but cached + actor-mediated.

10. **Delete dead code.**
    - `SyncJob` enum (replaced by `Effect`).
    - `QueuedSyncProvider` (replaced by Effect runtime).
    - `ping_peer.rs`, `sync_bucket.rs`, `download_pins.rs` (replaced by
      Effect handlers in coordinator).
    - `MountManager::on_bucket_synced` (replaced by VaultEvent
      subscriber).
    - Old `handle_message_side_effect` once all handlers are converted.

Risk: high. This is the user-visible behavior change point. Behaviors
to verify before deleting old code:

- Periodic pings still fire and trigger sync when out of sync.
- New-bucket discovery path (`on_new_bucket_discovered` in
  sync_bucket.rs:70) still fires.
- FUSE cache invalidation still happens.
- Backup sync still polls (or upgrades to event-driven).

### Phase 4 — push events + optimizations

11. **`HeadAdvanced` push after save.**
    - In `Vault::save`, emit `Effect::SendHeadAdvanced` for each
      share-holder.
    - Recipient handler enqueues `Effect::ApplyRemoteChain`.
    - Reduces ping latency from 5 min to ~RTT.

12. **Backup sync → event-driven.**
    - Subscribe to `VaultEvent::Saved`.
    - Remove the 30s poll loop.

13. **(Optional) iroh-gossip for vaults with N > threshold holders.**
    - Don't add until measured.

Risk: low. Pure additions over the new shape.

---

## §7 Open questions for decision

1. **Where does `VaultActor` live — zim-core or zim-peer?**
   - **zim-core**: Vault::spawn_actor() naturally lives next to Vault.
     But it imports tokio specifics (mpsc, JoinHandle), and any
     compile-time logic for VaultCommand variants ends up there.
   - **zim-peer**: Keeps zim-core focused on data types + algorithms,
     not runtime. VaultActor + VaultCommand live with the coordinator.
   - **Recommendation**: zim-core. Vault is async already; tokio is
     already a dep. The actor IS the entry point you wanted Vault to
     be; keeping them in the same crate is consistent. zim-peer holds
     `SyncCoordinator` and the wiring.

2. **Should `Effect` live in zim-core or zim-protocol?**
   - Effects span both layers (some are wire messages, some are local
     vault commands). Putting it in zim-core means zim-protocol depends
     on it (it already does). Putting it in zim-protocol means zim-core
     can't return Effects from `Vault::save`.
   - **Recommendation**: zim-core. Effects are the universal vocabulary;
     zim-protocol consumes wire-message variants and ignores the others.

3. **VaultCommand has many variants (~15). Is the boilerplate worth it?**
   - Alternative: `enum VaultCommand { Call(Box<dyn FnOnce(&mut Vault) -> Box<dyn Any>>) }` —
     just send closures. Saves the variant explosion but loses static
     checking.
   - **Recommendation**: keep enum. The variants are the API contract;
     making them explicit catches type drift early.

4. **Should `HeadRequest` and the existing `Ping` coexist or merge?**
   - Coexist (clean separation): one for "are we in sync" pings, one
     for direct query.
   - Merge (Ping becomes the only thing): add a `sample` field to Ping,
     reply includes ancestor.
   - **Recommendation**: coexist initially, revisit. Ping today is also
     the keep-alive; conflating semantics is risky.

5. **Does `apply_chain` save automatically?**
   - The current `sync_with` doesn't touch the working copy. After
     Phase 1, `apply_chain` should: log append + (if vault is open)
     merge + save. The save fires `VaultEvent::Saved` which propagates.
   - **Recommendation**: yes, save. The user wants one method that
     leaves the vault in a consistent state.

6. **What's the policy on conflicting concurrent saves?**
   - Two HTTP requests both want to write. With actor mailbox they
     serialize naturally. But what about save races between local
     save (user write) and apply_chain (sync)?
   - Today: same mutex, last-writer-wins on the head link, conflict
     resolution via op log.
   - Under VaultActor: same; the actor processes commands one at a
     time. Save and ApplyChain queue up and run sequentially. Net
     behavior unchanged.

7. **What about peer reputation / blacklisting after a failed sync?**
   - Out of scope for this refactor but worth noting. `Effect::SendPing`
     could be replaced/wrapped to consult a reputation table maintained
     by the coordinator.

---

## §8 Appendix: type sketches

### §8.1 The full Effect taxonomy

```rust
/// All side effects in the sync layer flow through this type.
#[derive(Debug, Clone)]
pub enum Effect {
    // -- Local vault commands --
    VaultCommand { id: Uuid, cmd: VaultCommand },
    ApplyRemoteChain {
        id: Uuid,
        target_link: Link,
        target_height: u64,
        peer_ids: Vec<PublicKey>,
    },
    DownloadBlobs {
        hashes: Vec<Hash>,
        peer_ids: Vec<PublicKey>,
    },

    // -- Wire messages --
    SendPing { peer_id: PublicKey, bucket_id: Uuid },
    SendHeadRequest { peer_id: PublicKey, bucket_id: Uuid },
    SendAncestorProbe {
        peer_id: PublicKey,
        bucket_id: Uuid,
        sample: Vec<(u64, Link)>,
    },
    SendHeadAdvanced {
        peer_id: PublicKey,
        bucket_id: Uuid,
        head: Link,
        height: u64,
    },

    // -- Event broadcasts --
    EmitVaultEvent(VaultEvent),
    EmitPeerEvent(PeerEvent),

    // -- Diagnostic --
    Log { level: tracing::Level, message: String },
}

#[derive(Debug, Clone)]
pub enum PeerEvent {
    PeerSeen { peer_id: PublicKey, bucket_id: Uuid },
    SyncStarted { bucket_id: Uuid, peer_id: PublicKey },
    SyncCompleted { bucket_id: Uuid, peer_id: PublicKey, height: u64 },
    SyncFailed { bucket_id: Uuid, peer_id: PublicKey, reason: String },
}
```

### §8.2 Coordinator surface

```rust
pub struct SyncCoordinator<L: VaultLog + 'static> {
    blobs: BlobsProvider,
    log_factory: Arc<dyn Fn() -> L + Send + Sync>,  // OR a per-coord log
    endpoint: Endpoint,
    secret: PrivateKey,
    vaults: RwLock<HashMap<Uuid, VaultActorHandle>>,
    peer_hints: RwLock<HashMap<Uuid, VecDeque<PublicKey>>>,
    vault_events: broadcast::Sender<VaultEvent>,
    peer_events: broadcast::Sender<PeerEvent>,
    effect_tx: mpsc::Sender<Effect>,        // self-dispatch queue
}

impl<L: VaultLog> SyncCoordinator<L> {
    pub async fn open_vault(&self, id: Uuid) -> Result<VaultActorHandle>;
    pub async fn pull_from(&self, id: Uuid, peer: PublicKey) -> Result<()>;
    pub async fn apply_effects(&self, effects: Vec<Effect>);
    pub fn subscribe_vault_events(&self) -> broadcast::Receiver<VaultEvent>;
    pub fn subscribe_peer_events(&self) -> broadcast::Receiver<PeerEvent>;

    // Wire-handler entrypoints (called from protocol message dispatch):
    pub async fn handle_head_request(&self, sender: PublicKey, msg: HeadRequest) -> HeadReply;
    pub async fn handle_ancestor_probe(&self, sender: PublicKey, msg: AncestorProbe) -> AncestorReply;
    pub async fn handle_head_advanced(&self, sender: PublicKey, msg: HeadAdvanced) -> Vec<Effect>;
}
```

### §8.3 Test shape (what becomes possible)

```rust
#[tokio::test]
async fn ping_behind_emits_apply_chain_effect() {
    let peer = test_peer().await;
    let req = PingMessage {
        bucket_id: BUCKET_X,
        link: REMOTE_HEAD,
        height: 42,
    };
    let reply = PingReply::behind(BUCKET_X, OUR_HEAD, 30);

    let effects = Ping::handle_message_effects(&peer, &SENDER_ID, &req, &reply).await;

    assert_eq!(effects.len(), 1);
    match &effects[0] {
        Effect::ApplyRemoteChain { id, target_link, target_height, peer_ids } => {
            assert_eq!(*id, BUCKET_X);
            assert_eq!(target_link, &REMOTE_HEAD);
            assert_eq!(*target_height, 42);
            assert!(peer_ids.contains(&SENDER_ID));
        }
        other => panic!("expected ApplyRemoteChain, got {other:?}"),
    }
}
```

No mocks, no broadcast channel inspection, no SyncProvider trickery.
Just a function call on a handler.

### §8.4 What happens to `SyncEvent`

`SyncEvent::MountInvalidated` becomes `PeerEvent::SyncCompleted`
(or stays as a separate FUSE-cache-invalidation signal — the FUSE
subscriber can listen to either).

`SyncEvent::BucketUpdated` becomes `VaultEvent::Saved` /
`VaultEvent::Synced` (which already exist after Pass-1).

The FUSE listener (`fuse_fs.rs:187 spawn_sync_listener`) becomes a
subscriber to `vault_events` instead of `sync_tx`. One channel, one
subscriber pattern.

---

## §9 What I did NOT investigate (flagging for awareness)

- **iroh-gossip integration cost.** I noted it as optional but didn't
  measure dep bloat or message-shape compatibility.
- **The `BucketLogProvider` trait vs `VaultLog` trait.** zim-protocol
  uses `BucketLogProvider`, zim-core has `VaultLog`. They look similar
  but I didn't diff them. The "vault" rename sweep that's
  half-finished is part of why they're different. Recommend unifying
  in Phase 2.
- **Auth boundary on the new messages.** `HeadRequest` and
  `AncestorProbe` don't decrypt anything, so any peer that knows the
  bucket UUID can ask. Is that intended? Same as today's Ping, so
  presumably yes — but worth confirming. `HeadAdvanced` could be
  filtered to share-holders only.
- **Backpressure semantics.** When `effect_tx` fills, what's the
  policy? `try_send` and drop? Block? Today's `QueuedSyncProvider`
  returns an error if the channel is full (sync_provider.rs:69–76).
- **Persistence of pending effects.** If the daemon crashes mid-sync,
  do we need to resume? Today: no, the next periodic ping recovers.
  Same answer post-refactor — unless we want at-least-once delivery,
  which is a much bigger ask.

---

## Decisions I want from you tomorrow

1. **Phasing.** Are you OK with Phase 1 (zim-core foundations) being
   the first work I pick up? It's all new code; low risk.
2. **VaultActor home.** §7.1 — vote zim-core or zim-peer.
3. **Effect home.** §7.2 — vote zim-core or zim-protocol.
4. **Ping coexistence.** §7.4 — coexist with new probe messages, or
   merge probe semantics into Ping?
5. **Backup sync.** Convert to event-driven in Phase 4, or keep poll
   for now?

Everything else I can decide as I go.
