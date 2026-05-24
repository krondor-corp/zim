# Access Model

How Zim decides who can read, who can write, and who only pins ciphertext. Consolidates protocol-level decisions that landed across T-006, T-008, T-016, T-001, and T-002.

Audience: contributors. End-user-shaped summary lives in `wiki/_docs/security.md` and the future `wiki/_docs/viewer-enrolment.md`.

## Two roles at the manifest-field layer

There is no `PrincipalRole` enum. The owner/mirror distinction is encoded as **two separate fields** on the manifest:

```rust
pub struct Manifest {
    // ...
    pub shares:  Vec<Share>,        // owners — hold a per-blob `Secret` envelope
    pub mirrors: Vec<PublicKey>,    // mirror peer keys — pin ciphertext only
    // ...
}
```

| Field | Capability | Holds plaintext? |
|---|---|---|
| `manifest.shares` | Read + write the bucket. Authorised by the existing owners (signed manifest append). | Yes — each `Share` includes a `SecretShare` envelope sealed for the share's pubkey. |
| `manifest.mirrors` | Pin and serve ciphertext blobs. Cannot decrypt. | No — mirrors never see a `Secret`. |

Authorisation is **possession of a working `SecretShare`**. The mirror entry grants nothing decryption-related; it only declares "this peer is allowed to fetch ciphertext from us and we'll fetch ciphertext from it."

Refs: T-006, T-006a, T-016 (closed); the proposal in `tasks/done/T-016.md ## Notes`.

## Per-file / per-folder publication

`Manifest::public: Option<Secret>` (whole-bucket plaintext key) is deleted. The publication surface is now per-entry:

```rust
pub struct Manifest {
    // ...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub published_set: Vec<PublicEntry>,
    // ...
}

pub struct PublicEntry {
    pub target: Link,
    pub secret: Secret,
    pub display_path: String,
    pub mode: PublishMode,        // File | Folder
}
```

- **Owners publish** specific files or folders by appending to `published_set`. Each entry carries the plaintext `Secret` for that target.
- **Auto-republish-on-commit** keeps published_set entries fresh as the tree changes (target hash refreshed, display_path updated on renames, deletions pruned).
- **Rotate ops** (`rotate_file`, `rotate_folder`) generate fresh `Secret`s — actual read revocation, not just access-list pruning.
- **Future**: a sibling `published_versions: Vec<PublicVersion>` field for whole-bucket-version publication is *not v1* but the schema is additive-safe (the `serde(default, skip_serializing_if = "Vec::is_empty")` on `published_set` confirms the wire format tolerates new optional fields).

Anonymous URL reads use the public branch of the envelope tagged-union; signed-in user reads use the sealed branch via the user's web-key.

Refs: T-008 (open; proposal in its Notes section).

## zim-hub is multi-tenant

One hub instance serves many Google-authenticated users — GitHub-style, not a single-user gateway. Implications:

- **Identity vault**: hub stores per-viewer encrypted private-key blobs keyed by Google `sub` (the `identity_keys` table). Plaintext keys never exist server-side; unlock happens in the browser via Argon2id-derived KEK in WASM. See [Identity & Key Custody](./identity.md).
- **Schema and routes namespace by user**: `(google_sub, bucket_id)` everywhere a `bucket_id` was previously assumed to be globally unique within the hub.
- **Each viewer's web-key is a full member** of `manifest.shares` (one entry per user-per-bucket, sealed for that viewer's web-key pubkey).

Refs: `broadcast/20260524T170810Z-thing2-multitenant-hub-framing.md`, T-001 (done), T-002 (in flight).

## zim-hub is auth-gated; unauthenticated visitors see marketing only

- `GET /` unauthenticated → marketing landing page. No bucket data leak.
- `GET /` authenticated → dashboard.
- `/b/{id}/*` and `/api/v0/buckets/*` require auth; unauthenticated → 302 to `/login`.
- Anonymous public-file reads (the surface produced by per-entry publication above, when T-008 lands) live on a **separate route surface** from `/b/*` — TBD by the T-008 sub-task that wires it in.

Refs: `broadcast/20260524T163814Z-hub-is-auth-gated-vault-not-public-browser.md`.

## Web key: a sign-only identity

A web-key is a **signing identity**, not a network peer:

- The web-key's secret half lives in the user's browser memory (Argon2id-unlocked from a hub-stored encrypted blob). The hub never sees plaintext.
- iroh peers need their private key available locally to authenticate QUIC connections. Since the hub doesn't hold the web-key's secret, **the hub cannot run an iroh peer on behalf of the user**.
- The web-key signs **content** (manifest appends, share envelopes for that key) but does not dial.

This is why the protocol needs a `dialable: bool` flag (next section): the share-membership model has to accept members that exist only to receive sealed envelopes.

## Hub as Mirror + Relay

The hub plays **two coordinated roles** in the protocol:

| Role | Mechanism | What it does |
|---|---|---|
| **Mirror peer** | The hub's own operator-side iroh key, registered in each bucket's `manifest.mirrors` | Pins ciphertext. Participates in the normal iroh sync mesh. Never holds bucket secrets. |
| **Relay** | The hub's HTTP API | Accepts signed manifest updates from browser sessions (`POST /api/v0/buckets/{id}/append`), validates the signature against the appending web-key, persists locally, and broadcasts to dialable peers via the normal iroh sync path. HTTP-in + iroh-out. No new wire verbs. |

The Relay role exists because web-keys can't dial. Without it, a browser-resident user could sign a manifest append but couldn't deliver it to the bucket's other peers. The hub bridges HTTP and iroh for those members.

A bucket can have multiple hubs (each in its own `mirrors` entry, each running independent Mirror+Relay). Owners with native peers don't need a hub at all; the hub is only required for browser-resident members.

Refs: `broadcast/20260524T182033Z-thing2-relay-role-dialable-shares-protocol-design.md` (protocol-precise version).

## `dialable: bool` on `Share`

```rust
pub struct Share {
    pub principal: Principal,
    pub envelope:  SecretShare,
    pub dialable:  bool,        // default: true
}
```

- **Default `true`** — most shares correspond to peers that have private-key access to their own iroh node. Sync layer's dial loop targets them.
- **`false`** for web-key shares — browser-resident members. Manifest updates reach them via the hub's Relay, not via iroh dial.

The flag affects the sync layer (dial filtering) only. Authorisation (whether a share is honored at all) ignores `dialable` — a non-dialable share still grants its holder the same read/write rights as any other share.

## Public bucket-version URLs (deferred)

v1 publication unit = file/folder via `PublicEntry`. Whole-bucket-version publication (GitHub-Pages-style — pin a manifest hash, expose its entire tree at a URL) is a future feature.

Design constraint preserved: the manifest schema's `published_set` field uses `#[serde(default, skip_serializing_if = "Vec::is_empty")]`, so a sibling `published_versions: Vec<PublicVersion>` field can be added later without a wire-format break.

Refs: `broadcast/20260524T170810Z-thing2-multitenant-hub-framing.md` ("deferred-future-feature" section).

## End-user-shaped summary (lives in wiki)

These are the three sentences that survive audience filters for the wiki:

- "Owners can read and write a bucket. Mirrors only hold encrypted copies — they can't see your data."
- "You can publish individual files or folders. Anyone with the link can read those; everything else stays private."
- "Sign in via Google. Your web-key unlocks in your browser only — even the hub admin can't see it."
- "Editing through the web works exactly like editing on your laptop or phone. The hub passes your signed changes to your other devices."

The wiki write-up of these lands when T-001a M4 ships (the unlock UX is the shipping-UX reference for `wiki/_docs/viewer-enrolment.md`).

## Related concepts

- [Identity & Key Custody](./identity.md) — vault-not-custodian pattern, Argon2id+ChaCha20, threat model.
- [Security](./security.md) — bucket-level threat model and protocol invariants.
- [Cryptography](./cryptography.md) — `Secret`, `Share`, ChaCha20-Poly1305, X25519, BLAKE3.
- [Data Model](./data-model.md) — `Manifest.shares`, per-blob `SecretShare`, `published_set`.
- [Synchronization](./synchronization.md) — iroh sync mesh, mirror peer-type, append-only bucket log.
