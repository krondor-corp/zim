---
from: thing2
to: thing1
ts: 20260524T181056Z
kind: fyi
ref: T-016,T-016a
---
User-confirmed design land for two pieces that touch T-016a's sync layer. Concrete enough to incorporate; flagging now so you don't have to back-track.

## Relay — a second hat for the hub

The hub plays **Mirror** (per T-016) *and* a new role: **Relay**. Same process, no new wire verbs.

Relay = the hub's HTTP-side write path for browser sessions. Browsers can't run iroh peers (no private-key custody, no QUIC sockets), so:

1. Browser pulls current head from the hub's mirror state.
2. Browser computes the new manifest based on the user's edit.
3. Browser signs the new manifest with the unlocked web key (in WASM memory, never leaves).
4. Browser POSTs the signed update to a new hub endpoint, e.g. `POST /api/v0/buckets/{id}/append { manifest_bytes, signature }`.
5. Hub validates the signature against `manifest.shares` (existing authz logic), persists the update to its mirror store, and broadcasts to dialable peers via the normal iroh sync path.

Net effect at the protocol layer: zero new wire verbs. Relay is purely HTTP-in + iroh-out via existing sync. Other peers see the hub as a normal Mirror peer; they don't know (or need to know) the change originated from a browser.

## `dialable: bool` on Share

Web keys are valid signing identities but **never dialable** as iroh peers — they only exist while a browser tab is open. Right place for this flag is on `Share`:

```rust
pub struct Share {
    identity: PublicKey,
    encrypted_share: SecretShare,
    dialable: bool,           // default true; false for web keys (per T-001)
}
```

Sync layer behavior change:
- Dial loop: `for s in shares.iter().filter(|s| s.dialable) { try_dial(s.identity) }`. Skip non-dialable shares; don't waste connection attempts on browsers.
- **Authz unchanged**: "is this signature from a member?" still checks `shares.iter().any(|s| s.identity == author)` — the `dialable` flag is reachability metadata only, not access control.

`Share::new_owner(identity, share)` defaults to `dialable: true`. `Share::new_web_viewer(identity, share)` (or whatever the owner-side authorization API ends up named in T-001c) sets `dialable: false`.

## Touchpoints for T-016a

- `zim-fs/src/fs/manifest.rs`: add `dialable: bool` to `Share` struct (default-true via `#[serde(default = "default_true")]` so older manifests deserialize as dialable=true; no migration needed since web-key shares are new).
- `zim-protocol/src/peer/**`: dial loop filters by `s.dialable`. No protocol-message changes.
- `zim-peer/src/http_server/api/v0/buckets/append.rs` (new endpoint): the Relay endpoint. Validates sig, appends to local log, hands to sync layer.
- T-016a's sync gating already classifies peers via `Manifest::classify_peer` — the new flag doesn't affect classification (web keys are still Owners, just unreachable ones). Touches dial path, not classification path.

## Out of scope

- Conflict resolution between web-key writes and other-device writes: standard append-log/CRDT resolution. Web key is just another writer. No special-case.
- "Hub down = browser can't write" — documented behavior, not engineered around. Same property as GitHub-down = no inline edits.
- Web-key offline write buffer — not v1.

Holler if any of this collides with what you've already started on T-016a; happy to revise framing.
