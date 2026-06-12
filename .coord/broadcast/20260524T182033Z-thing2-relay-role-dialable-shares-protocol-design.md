---
from: thing2
ts: 20260524T182033Z
kind: protocol-design
audience: all
ref: T-016,T-001,T-008
supersedes: nothing; extends T-016 (closed) into the HTTP-relay layer
---

# Protocol design — Relay role, `dialable` shares, web-key invariants

This formalizes a protocol design that emerged from today's user exchange. Extends T-016 (Mirror peer-type) into the write path for browser-resident identities.

## Motivation

T-001 + T-016 left a gap. T-001 says the hub never holds plaintext web-keys (encrypted-blob-on-hub, browser-side Argon2id unlock). T-016 says the hub is a Mirror peer (operator iroh key in `manifest.mirrors`, pins ciphertext, no decryption). But T-001 also says the web-key is a full member of `manifest.shares` — it must be able to **author writes**, not just read.

Browsers can't run iroh peers (no QUIC sockets, no place to safely custody an iroh private key long-term, hub never sees the web-key plaintext anyway). So a web-key member can sign manifest updates but cannot itself dial peers or accept dials. Without a bridge, web-key writes are stuck.

This design adds that bridge: a **Relay** role on the hub for HTTP-side write ingestion, plus a `dialable: bool` field on `Share` that distinguishes signing-capable identities from network-reachable ones.

## Non-goals

- No new wire (iroh) protocol verbs. Relay is purely HTTP-in + existing-iroh-out.
- No change to T-006 (no `PrincipalRole` enum). Role distinction remains at the manifest-field layer.
- No change to T-016's `manifest.mirrors` semantics.
- No offline-write buffer in the browser (browser-online == hub-online is a documented constraint).
- No conflict-resolution change. Web-key writes are just another writer in the append-log.

## Design

### 1. `Share` gains a `dialable: bool` field

```rust
// crates/zim-fs/src/fs/manifest.rs

pub struct Share {
    pub identity: PublicKey,
    pub encrypted_share: SecretShare,
    #[serde(default = "default_true")]
    pub dialable: bool,
}

fn default_true() -> bool { true }
```

Semantics:

- **`dialable: true` (default)** — this share's identity runs an iroh peer somewhere reachable. Sync layer should try to dial.
- **`dialable: false`** — this share's identity is a signing-only key (e.g., a browser-resident web-key). Sync layer must not attempt to dial. Manifests authored by this identity arrive via Relay (HTTP), not via direct iroh sync from this identity.

The flag is **reachability metadata only**. It is NOT access control:

- Authz check (verifying a signature on a manifest update): `manifest.shares.iter().any(|s| s.identity == author_pubkey)`. The `dialable` flag is ignored.
- Decryption capability: every Share carries an `encrypted_share`. The `dialable` flag does not affect whether the share contains a secret.

`#[serde(default = "default_true")]` means existing manifests deserialize cleanly (all pre-flag shares are dialable). No migration step needed.

### 2. Sync layer filters its dial loop

In `crates/zim-protocol/src/peer/**`, wherever the sync layer iterates `manifest.shares` to attempt connections:

```rust
for share in manifest.shares.iter().filter(|s| s.dialable) {
    try_dial(&share.identity).await;
}
```

Identities in `manifest.mirrors` are always dialable (mirrors are real iroh peers by definition). The `dialable` flag only lives on `Share`.

No protocol-message changes. The dialing behavior is local to each peer; no wire format involved.

### 3. Hub gains a Relay role

The hub already plays Mirror (per T-016, hub's iroh peer key in `manifest.mirrors`). It additionally accepts a new HTTP endpoint for write ingestion:

```
POST /api/v0/buckets/{bucket_id}/append
Content-Type: application/cbor (or JSON, decide at impl time)
Body: { manifest_bytes: Vec<u8>, signature: Signature }
```

Behavior:

1. **Parse** `manifest_bytes` as a `Manifest`. Reject if malformed (400).
2. **Lineage check**: the new manifest's `prev_head` (or equivalent) must match the hub's current head for this bucket. Reject if not (409 Conflict — browser must pull + re-sign).
3. **Signature check**: verify `signature` against `author_pubkey ∈ manifest.shares`. Reject if not a member or signature is invalid (403).
4. **Persist**: append to the hub's local mirror state (same store the iroh sync layer writes into).
5. **Broadcast**: feed the new manifest into the same outbound-sync code path the iroh layer uses. Dialable peers receive it via normal sync.
6. **Respond** with the new head's link (200).

The Relay endpoint reuses existing manifest-validation and outbound-sync code paths. It does not introduce a parallel write path through the bucket log — it injects HTTP writes into the same log the iroh write path uses.

### 4. Web-key invariants

A web-key Share in `manifest.shares`:

- `identity` = the user's ed25519 pubkey, generated browser-side at enrolment (per T-001).
- `encrypted_share` = the bucket's per-blob secret material, X25519-sealed to the user's pubkey. Generated owner-side when authorizing the viewer.
- `dialable: false`.

A web-key:

- **Reads** by fetching ciphertext from the hub (HTTPS) and decrypting in browser WASM after Argon2id-unlocking the local private key.
- **Writes** by computing the new manifest in browser WASM, signing with the in-memory private key, and POSTing to the hub's Relay endpoint.
- **Never** runs an iroh peer.
- **Never** appears in any iroh dial list anywhere in the system.

### 5. Authorization API change (cross-task hint)

The existing owner-side "authorize this viewer" CLI/HTTP (T-001c, open) needs to learn one new flag:

```
zim viewer authorize <pk> --bucket <id> --web-key
# Authorizes <pk> as a Share with dialable=false (default for --web-key).
# Without the flag: dialable=true (treats <pk> as a regular peer).
```

T-001c executor: please incorporate. This is a one-line change in the CLI op and a one-field change in the HTTP request body.

## Schema diff (for the implementer)

### Before
```rust
pub struct Share {
    pub identity: PublicKey,
    pub encrypted_share: SecretShare,
}
```

### After
```rust
pub struct Share {
    pub identity: PublicKey,
    pub encrypted_share: SecretShare,
    #[serde(default = "default_true")]
    pub dialable: bool,
}

fn default_true() -> bool { true }
```

That's the entire on-disk schema delta. The Relay HTTP endpoint is a new file, no schema implication.

## Touchpoints

### `crates/zim-fs/src/fs/manifest.rs` (T-016a / owner: thing1)
- Add `dialable: bool` field to `Share` with `#[serde(default)]`.
- Add `default_true()` helper.
- `Share::new_owner(identity, share)` — defaults `dialable: true`.
- New constructor `Share::new_web_viewer(identity, share)` — sets `dialable: false`.

### `crates/zim-protocol/src/peer/**` (T-016a / owner: thing1)
- Sync dial loop: filter `manifest.shares` by `.dialable`. Wherever a `try_dial` is wired to share identities, insert the filter.
- `Manifest::classify_peer` (per T-016) is unaffected. Web-keys are still classified Owner — they're just unreachable Owners.

### `crates/zim-peer/src/http_server/api/v0/buckets/append.rs` (new file / owner: thing1 or T-008b worker — whoever owns hub HTTP endpoints)
- The Relay endpoint as specified in §3. Reuses existing manifest-validation + sync-broadcast code.

### `crates/zim-peer/src/cli/ops/viewer/authorize.rs` (T-001c / owner: thing1 or whoever picks T-001c up)
- Add `--web-key` flag. When present: `Share::new_web_viewer(...)`. When absent: `Share::new_owner(...)`.

### `crates/zim-peer/src/http_server/api/v0/viewer/authorize.rs` (T-001c)
- Request body gains an optional `dialable: bool` field (default `true`).

### `crates/zim-hub/**` (T-001a / owner: thing3)
- Hub's enrolment flow (per T-001) — already calls `zim viewer authorize` against owner-side. Pass `--web-key` when authorizing a web-key viewer. No new code, just the flag.

## Acceptance criteria (for whichever sub-task implements this)

- [ ] `Share` struct has the `dialable: bool` field with `#[serde(default = "default_true")]`.
- [ ] Existing manifests in-tree (test fixtures, dev data) deserialize cleanly into the new shape with `dialable: true`.
- [ ] Sync layer's dial loop filters by `dialable`.
- [ ] `Manifest::classify_peer` is unchanged.
- [ ] Authz check ("is signature from a member?") is unchanged — uses `shares.contains(author)` only.
- [ ] New HTTP endpoint `POST /api/v0/buckets/{id}/append` accepts a signed manifest update, validates lineage + signature + membership, persists, and broadcasts via existing sync path.
- [ ] Endpoint returns 409 on stale `prev_head`, 403 on non-member or invalid signature, 400 on malformed body, 200 with new head link on success.
- [ ] `zim viewer authorize` gains `--web-key` flag that sets the resulting Share's `dialable: false`.
- [ ] Test: a Share with `dialable: false` can sign an append-log entry that other members accept.
- [ ] Test: sync layer does not attempt to dial any identity from a `dialable: false` Share.
- [ ] Test: Relay endpoint round-trip — a signed manifest POSTed through Relay reaches a dialable peer over iroh sync.
- [ ] `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --all -- --check` all green.

## Open questions

1. **Endpoint body encoding** — CBOR (matches on-the-wire format) or JSON (matches the rest of zim-hub's API surface)? Suggest CBOR for the manifest_bytes payload (it's already what zim-fs serializes to) with a thin JSON envelope. Implementer's call.

2. **Rate-limiting Relay writes** — out of scope here, but the hub will need it (web-key writes are user-driven, can be abused). Flag for thing3's hub-side work.

3. **`zim_protocol::Peer` API surface for "ingest external manifest"** — the existing append path probably has a local-write-only API. Relay needs to call a slightly different entry point that accepts an externally-signed manifest. Whoever picks T-016a should decide whether this is a new method on `Peer` or a generalization of the existing append.

4. **Migration of existing test fixtures** — if any test data file embeds a `Share` without `dialable`, the `#[serde(default)]` handles it. But if there are existing Share constructors in test code with positional args, those need adjustment. Implementer's grep job, not a design call.

## What this does NOT change

- Mirror peer-type (T-016): unchanged. `manifest.mirrors` still a `Vec<PublicKey>`. Hub still added as Mirror per-bucket.
- `Manifest::public` deletion (T-008): unchanged. Per-file/folder publication via `PublicEntry`/`published_set` is orthogonal.
- T-001 identity-vault design: unchanged. Encrypted blob storage, Argon2id browser unlock, key never leaves browser.
- Multi-tenant hub framing (broadcast 20260524T170810Z): unchanged. Multi-tenancy lives at the HTTP/session/membership layer; this broadcast extends the same layer.
- Auth-gated-vault policy (broadcast 20260524T163814Z): unchanged. Relay endpoint requires session auth like all other `/api/v0/*` endpoints.

— thing2
