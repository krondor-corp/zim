# Data Model

Core data structures in Zim: Buckets, Manifests, Nodes, Shares, Publication, Pins, and the Bucket Log.

All filesystem types live in `crates/zim-core/src/fs/`.

## Buckets

A **bucket** is a versioned, encrypted collection of files and directories. Each bucket is identified by a `VaultId` — the BLAKE3 hash of its genesis manifest blob. Identity is *derived*, never declared: manifests carry no id field, so whether a chain belongs to a vault is decided by walking `previous` links back to genesis and hashing it. Genesis carries a random nonce so identical-content vaults still get distinct ids. Each bucket contains:

- **Manifest** — current state (unencrypted metadata)
- **Root Node** — encrypted directory tree
- **Blobs** — encrypted file contents
- **Version chain** — link to previous manifest, forming an immutable audit trail

## Manifests

The manifest is the entry point to a bucket.

Location: `crates/zim-core/src/fs/manifest.rs`

```rust
pub struct Manifest {
    nonce: [u8; 16],
    name: String,
    height: u64,
    version: Version,
    shares: Shares,
    relays: Vec<Relay>,
    entry: Link,
    pins: Link,
    previous: Link,
    ops: Link,
    published: Published,
}
```

| Field | Purpose |
|---|---|
| `nonce` | Random salt minted at genesis, carried forward by saves. Makes the derived `VaultId` (hash of the genesis blob) unique even for identical-content vaults. |
| `name` | Human-readable label (not guaranteed unique). |
| `height` | Monotonically increasing version number. Genesis = 0. |
| `version` | Software version that created this manifest. |
| `shares` | Map of `PublicKey → Share` — who can decrypt this bucket. See [Shares](#shares). |
| `relays` | Peer keys that pin ciphertext but cannot decrypt (hub/mirror peers). See [Relays](#relays). |
| `entry` | CID pointing to the encrypted root `Node`. |
| `pins` | CID pointing to a `Pins` blob hash set (content to keep locally). |
| `previous` | CID of the prior manifest version (default for genesis). |
| `ops` | CID of the encrypted CRDT path-operations log. Default if empty. |
| `published` | Per-file/folder publication map. See [Publication](#publication). |

Serialized as DAG-CBOR, stored as blobs in iroh's content-addressed store, addressed by BLAKE3 hash.

## Nodes

A **node** represents a directory. Nodes are encrypted and content-addressed.

Location: `crates/zim-core/src/fs/node.rs`

```rust
pub struct Node(BTreeMap<String, Leaf>);

pub enum Leaf {
    File {
        link: Link,
        secret: Secret,
        mime: MaybeMime,
        metadata: MaybeMetadata,
    },
    Directory {
        link: Link,
        secret: Secret,
    },
}
```

Each entry in a node maps a name (e.g. `"README.md"`, `"src"`) to a `Leaf`:

- **`Leaf::File`** — content-addressed pointer to an encrypted file blob, plus the per-file encryption key, MIME type, and optional metadata.
- **`Leaf::Directory`** — pointer to a child `Node`, plus the encryption key for that node.

Every file and directory has its own `Secret`. This enables fine-grained encryption, efficient key rotation, and stable content-addressed hashes.

**Encryption flow:** Node → serialize to DAG-CBOR → encrypt with ChaCha20-Poly1305 using the node's secret → store as blob → address by BLAKE3 hash of ciphertext.

## Shares

A `Share` represents a member's access to a bucket.

Location: `crates/zim-core/src/fs/share.rs`

```rust
pub struct Share {
    identity: PublicKey,
    secret_share: SecretShare,
    dialable: bool,  // default: true
}
```

| Field | Purpose |
|---|---|
| `identity` | Ed25519 public key of the member. |
| `secret_share` | The bucket secret, wrapped (ECDH + AES-KW) for this member's key. |
| `dialable` | `true` for native peers (CLI, daemon); `false` for browser-resident web-keys. The sync dial loop skips non-dialable shares. Authorisation ignores this flag — a non-dialable share grants the same read/write rights. |

Members with `dialable: false` are browser-resident identities (unlocked via zim-wasm Argon2id flow). Their manifest updates reach the network via the hub's Relay role, not via direct iroh dial.

See [Access Model](./access-model.md) for the full owner/relay/web-key model.

## Relays

A `Relay` is a peer that pins ciphertext without holding the bucket secret.

```rust
pub struct Relay {
    identity: PublicKey,
}
```

Relay peers (typically zim-hub instances) are listed in `manifest.relays`. They:

- Fetch and pin encrypted blobs (manifests, nodes, file content).
- Serve published content to authenticated viewers.
- Accept and forward browser-signed manifest appends (the Relay role).
- **Cannot decrypt** any bucket content — they never hold a `SecretShare`.

The bucket owner adds a relay via `zim bucket mirror add <bucket> <relay-pubkey>`.

Wire-format note: the field is serialized as `relays` with a `#[serde(alias = "mirrors")]` backward-compat alias.

## Publication

Per-file/folder publication replaces the old whole-bucket `public: Option<Secret>`.

Location: `crates/zim-core/src/fs/published.rs`

```rust
pub type Published = BTreeMap<AbsPath, Leaf>;
```

Each entry maps an absolute path in the bucket to the current `Leaf` at that path. The `Leaf`'s `link` + `secret` allow anyone with the published map to fetch and decrypt that specific file or directory — without holding the bucket-wide secret.

- **Auto-refreshed on save** — when the bucket is saved, stale published paths are pruned and surviving entries are updated to point at the current `Leaf`.
- **Rotation** — calling `rotate_file` / `rotate_folder` generates a fresh `Secret` for the target, achieving actual read revocation (old `Leaf` secrets stop working).
- **Wire format** — `#[serde(default, skip_serializing_if = "Published::is_empty")]` so the field is absent from manifests with no published content.

See [Access Model](./access-model.md) for the full publication design.

## Pins

Pins define which content to keep locally and prevent garbage collection.

Location: `crates/zim-core/src/fs/pins.rs`

```rust
pub struct Pins(HashSet<Hash>);
```

A set of BLAKE3 hashes representing blobs to retain. Serialized as an iroh HashSeq, stored as a blob, linked from the manifest's `pins` field.

When saving: collect all node + file blob hashes → add to pins → serialize → store → manifest points to it.
When syncing: download the pins HashSeq → verify all pinned content is available → download missing blobs from peers.

## Bucket Log

A height-based version control system tracking all versions of a bucket, including divergent forks.

Location: `crates/zim-protocol/src/log/`

Each peer maintains a local log mapping `bucket_id → height → Vec<Link>`:

- **Height** — monotonically increasing. Genesis = 0, `previous = default`. Each subsequent version = `parent_height + 1`.
- **Multiple heads** — forks are multiple links at the same height. Canonical head = max link by hash comparison (deterministic across all peers).
- **DAG structure** — each manifest's `previous` points to its parent. The log validates `previous` exists at `height - 1` before appending.

### Validation rules

1. If `previous` is non-default, it must exist at `height - 1`.
2. If `previous` is default (genesis), `height` must be 0.
3. Same link cannot appear twice at the same height.
4. During sync: the peer providing the update must be in the manifest's shares.

### Sync integration

1. **Height comparison** — peers exchange heights to detect divergence.
2. **Ancestor finding** — walk the manifest chain backward to find a common ancestor.
3. **Chain download** — download manifests from target back to ancestor.
4. **Log application** — append to local log; forks auto-detected and stored.

All peers converge to the same canonical head through deterministic fork resolution.

## Related concepts

- [Access Model](./access-model.md) — owner/relay/web-key roles, publication design, hub as Mirror + Relay.
- [Identity](./identity.md) — viewer key custody, Argon2id unlock, device model.
- [Cryptography](./cryptography.md) — Ed25519, ChaCha20-Poly1305, X25519 ECDH, BLAKE3.
- [Synchronization](./synchronization.md) — sync protocol, peer structure, dial loop.
