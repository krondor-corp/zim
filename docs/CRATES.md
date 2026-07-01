# Zim Crate Layout

The shipped shape of the Zim workspace. 8 crates: 2 binaries (`zim`, `zim-hub`), 6 libraries.

## Workspace

```
crates/
├── zim-crypto/    # Ed25519/X25519 identity, ChaCha20-Poly1305, X25519 secret sharing
├── zim-did/       # DID types: did:key (daemon) + did:web (hub/user) built on zim-crypto
├── zim-core/      # Core: filesystem, content store, linked data, iroh abstraction, vault log + peer store traits
├── zim-runtime/   # Service trait + ShutdownHandle (shared lifecycle plumbing) — leaf crate
├── zim-peer/      # Sync orchestration, concrete trait impls, iroh transport, daemon binary `zim` (also a library)
├── zim-hub/       # Read-only web mirror gateway: ciphertext + log mirror, browser-side decryption, Google OAuth
└── zim-wasm/      # Browser-side WASM client for zim-hub (client-side decrypt of published blobs)
```

`_zim-protocol/` and `_zim-peer/` (underscore-prefixed) are archived on disk but excluded from the workspace — they are the pre-refactor implementations, kept for reference while the replacement stabilises.

## Dependency graph

Strict DAG. Arrows mean "depends on" (Cargo `path` dep).

```
zim-wasm  ─→ zim-crypto
           └─→ zim-core (dev-only: roundtrip tests against real encoder)

zim (binary) ─┬─→ zim-peer
              ├─→ zim-core
              ├─→ zim-crypto
              ├─→ zim-did
              └─→ zim-runtime

zim-hub   ─┬─→ zim-peer (default-features = false, excludes FUSE)
           ├─→ zim-core
           ├─→ zim-crypto
           ├─→ zim-did
           └─→ zim-runtime

zim-peer  ─┬─→ zim-core
           ├─→ zim-crypto
           ├─→ zim-did
           └─→ zim-runtime

zim-core ─┬─→ zim-crypto
          └─→ zim-did

zim-did  ─→ zim-crypto

zim-crypto:  leaf (no workspace deps)
zim-runtime: leaf (no workspace deps)
```

## Crate responsibilities

### `zim-crypto`

Wraps `iroh::PublicKey`/`SecretKey` by default (`iroh-keys` feature). The `wasm` feature swaps to `ed25519-dalek` directly so the crate compiles to `wasm32-unknown-unknown` for `zim-wasm`.

Provides: Ed25519 key generation/signing/verification, X25519 ECDH, ChaCha20-Poly1305 AEAD (`Secret`, `SecretShare`), streaming cipher (`encrypt_reader` / `decrypt_reader` for file content).

### `zim-did`

DID (Decentralized Identifier) types for Zim. Separates logical identity from raw key material so users with multiple devices can be addressed as a single `Identity`.

- `did:key:<multibase>` — daemon self-describing identity (encodes the pubkey directly).
- `did:web:hub.example.com` — hub operator identity, resolves over HTTPS.
- `did:web:hub.example.com:u:alice` — user identity listing multiple verification methods.

Provides: `Identity` enum, `HttpDidResolver`, DID-to-pubkey resolution.

### `zim-core`

Central shared library. Everything that is neither crypto primitives nor network I/O lives here.

Sub-modules:

| Module | Contents |
|---|---|
| `fs` | `Manifest`, `Node`, `Leaf`, `Share`, `Relay`, `Published`, `Pins` — filesystem-level types; CRDT path ops and conflict resolution |
| `blobs` | `BlobStore` trait, `BlobsProvider` (`Arc<iroh_blobs::BlobsProtocol>`), `legacy_fs` constructor |
| `linked_data` | `Link`, `Hash`, `Cid`, `BlockEncoded`, DAG-CBOR codec helpers |
| `iroh` | thin re-exports of iroh types used across the workspace |
| `vault` | `Vault<B, L>` struct (generic over blob store + log), `VaultId` (self-certifying: `blake3(genesis blob)`), `VaultLog` trait, `Head { link, height }` |
| `peers` | `PeerStore` trait, `PeerEntry`, `PeerStoreError` |

`VaultId` is self-certifying: it is the blake3 hash of the genesis manifest blob. Genesis manifests include a random 16-byte nonce so two vaults with identical content get distinct ids. This eliminates the forgeable `id: Uuid` field that existed on `Manifest` pre-refactor.

### `zim-runtime`

Leaf crate. `Service` trait + `ShutdownHandle`. Both binaries register named services and `.wait()` for SIGINT/SIGTERM, which fans the shutdown signal out to all registered handles.

### `zim-peer`

Server-side / iroh-coupled implementation layer. **Not usable in wasm** — depends on iroh's native transport.

Sub-modules:

| Module | Contents |
|---|---|
| `messages` | Wire-level request/reply structs (`Head`, `Probe`, `AncestorReply`, `Ping`, `ShareOffered`, `HeadAdvanced`) |
| `effect` | `Effect` enum — side-effect taxonomy for background work dispatched by the coordinator |
| `coordinator` | `SyncCoordinator` — `open_vault`, `sync_vault`, `apply_chain`; background effect runner; genesis verification |
| `iroh_transport` | iroh `ProtocolHandler` impl; routes QUIC frames to the coordinator |
| `relay_pull` | hub-mirror log-only chain pull (hub embeds a peer as a relay, not a shareholder) |
| `chain` | manifest-chain walk + DAG ops merge primitives |
| `log` | `SqliteVaultLog` + `MemoryVaultLog` — concrete `VaultLog` impls |
| `object_store` | SQLite-indexed local-dir and S3 backends, bridged to iroh via `ObjectStoreActor`; exposes `local_provider`, `s3_provider`, `provider_from` |
| `peers` | `MemoryPeerStore` — in-memory `PeerStore` impl (tests + hub); daemon uses `TomlPeerStore` in `crates/zim/src/peers.rs` |

`Vault<L>` in this crate is a **type alias**: `pub type Vault<L> = zim_core::vault::Vault<BlobsProvider, L>`. There is no wrapper struct.

`SyncCoordinator` performs genesis verification in `apply_chain`: when walking a chain to genesis, it asserts `VaultId::from_genesis_link(first_link) == claimed_vault_id`. Hijacked chains are rejected before any log append.

The `Peer` struct owns a `SyncCoordinator`, the iroh `Endpoint`, and a `Router`. The `router: Mutex<Option<Router>>` / `runner: Mutex<Option<JoinHandle<()>>>` pattern is deliberate: `Router::shutdown` consumes `self` and `JoinHandle::await` is one-shot, so `Option::take()` provides consume-on-shutdown semantics while `Mutex` allows concurrent shutdown from multiple `Arc` clones on a multithreaded runtime.

### `zim-hub`

Binary `zim-hub`. Read-only web mirror gateway.

- Embeds a `Peer` as a relay (never a shareholder — `ShareNotFound` on every mirrored vault is expected and harmless).
- Serves ciphertext blobs + vault log over HTTP; browser-side `WasmVault` in `zim-wasm` decrypts.
- Auth: enrolled daemons send self-signed EdDSA JWTs (`alg=EdDSA, kid=<pubkey_hex>`, 60s TTL, audience = hub URL). Hub verifies against pubkeys enrolled in `user_peers`.
- Blob storage: `ZIM_HUB_S3_*` env vars select minio (dev) or real S3 (prod). `bin/hub` and `make hub` start minio automatically.
- Templates: Askama (server-rendered HTML). Hypermedia: Datastar (not HTMX).

### `zim-wasm`

Compiled to `wasm32-unknown-unknown` via `wasm-pack`. Loaded by zim-hub pages.

- `WasmVault`: opens a vault manifest in-browser, recovers the root secret from the session key's `SecretShare`, exposes `readRootDir`, `readDir`, `readFile`.
- Dir bodies: one-shot AEAD (`Secret::decrypt`).
- File bodies: streaming cipher (`Secret::decrypt_reader` = 12-byte nonce || ChaCha20 XOR). **Not** AEAD — integrity comes from the `plaintext_hash` field on the `Entry::File`.
- Key management: `generate_key`, `encrypt_key_blob`, `unlock_key_blob` for the escrow/IndexedDB identity-vault flow.

## Naming conventions

| Aspect | Convention |
|---|---|
| Crate directory | `crates/zim-<name>/` (kebab-case) |
| Cargo package `name` | `zim-<name>` (kebab-case) |
| Library Rust import path | `zim_<name>::` (snake_case) |
| Binaries | `zim` (in `zim-peer`), `zim-hub` (in `zim-hub`) |
| Archived crates | `crates/_zim-<name>/` (underscore prefix, excluded from workspace) |
| Versions | All pre-1.0. No semver continuity to the previous `jax-*` crates. |

## What this doc does not cover

- Internal API design inside any crate — see `docs/architecture/project-layout.md`.
- HTTP surface details — `docs/reference/api.md` and `docs/reference/cli.md`.
- Database schema — migrations in `crates/zim-peer/migrations/` and `crates/zim-hub/migrations/`.
- Feature gates — `zim-peer`'s `fuse` feature, `zim-crypto`'s `iroh-keys`/`wasm` features — documented at the crate level.
