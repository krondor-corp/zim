# Acceptance and sharing

> **Status: implemented.** The address book is decoupled from the sync
> protocol: `SyncCoordinator<L>` / `Peer<L>` carry no `PeerStore` generic and
> hold an `Arc<dyn AcceptPolicy>`; `HeadAdvanced` carries a `recipient`; the
> daemon and hub each supply their own policy; share population
> (`reconcile_trusted`) lives in the daemon. One deferral remains, called out
> below.

## The principle

The sync protocol does not need an address book. It needs a **decision**:
*"should I accept this vault push?"* Everything about *where that decision's
data comes from* — a contacts table, a multi-user device registry, a test
stub — is not the protocol's business.

Two concerns were leaking into the sync layer and must move out:

1. **Share population (outbound)** — when creating or updating a vault,
   deciding *who* gets a share. This is daemon orchestration that *calls into*
   the protocol (`announce_head`) and `zim-core` (`add_reach` / `save`); the
   protocol only transports the resulting manifest. It never decides shares.

2. **Acceptance (inbound)** — when a push arrives, deciding whether to take
   it. This is a single yes/no **hook** the protocol calls, not a store it
   owns.

Concretely: `SyncCoordinator` loses its `P: PeerStore` generic. `VaultLog`
(`L`) stays a generic — it is *hot* (every chain op) and fundamental. The
acceptance decision becomes a *cold* trait object (`Arc<dyn AcceptPolicy>`),
called once per incoming push. The asymmetry is deliberate: different role,
different call frequency, different mechanism.

## The acceptance hook

```rust
#[async_trait]
pub trait AcceptPolicy: Send + Sync + 'static {
    /// Accept this incoming vault push?
    async fn accept_sync(&self, s: &IncomingSync) -> bool;
}

pub struct IncomingSync {
    /// Who dialed us and pushed.
    pub sender: PublicKey,
    /// The shareholder client this push is *for* — equal to the receiver
    /// for a direct peer, a hosted device for a relay. See "Recipient".
    pub recipient: PublicKey,
    pub vault_id: VaultId,
    /// Whether we already mirror this vault. Lets a policy fast-path
    /// known vaults without re-gating.
    pub known_vault: bool,
}
```

`SyncCoordinator::handle_head_advanced` calls `accept_sync` on **every**
`HeadAdvanced` and drops the push when it returns `false`. The coordinator
holds one `Arc<dyn AcceptPolicy>`, supplied at construction by whichever
binary built it. It owns no contacts table and no DID resolver.

### Why `recipient` — the relay case

A direct peer is its own recipient: when Alice shares straight to Bob's
daemon, Alice dials Bob, and *who it's for* == *who received it* == Bob. A
daemon's policy therefore never needs a separate recipient.

A **relay breaks that identity.** When Alice's daemon announces to the *hub*
because Carol's browser holds a share routed `via=hub`, the receiver (hub) is
**not** the recipient (Carol). The hub is multi-tenant — it cannot answer
"should I store this?" from the sender alone, because the sender says nothing
about *whose* storage this is. It must be told: *"I'm delivering this to you
on behalf of Carol."* So the recipient travels on the wire (see "Wire
change") and the hub gates on it.

`recipient` is only non-redundant for relays — which is precisely why it is
"required for the hub."

### Why it fires on every push, not just new vaults

A first-time share to a *new* recipient lands as an **advance to an existing
vault**, not as a new vault. If the recipient were only inspected when a vault
is first seen, the hub would silently miss it: the new device would never be
registered, and could never read that (old) vault from the hub.

So `accept_sync` runs on every `HeadAdvanced`. The `known_vault` flag lets a
policy choose whether to re-gate:

- The **daemon** fast-paths known vaults (it need not re-gate a peer it has
  already accepted from), so its behavior is unchanged.
- The **hub** ignores `known_vault` and re-checks every time, so a recipient
  freshly added to an old vault is caught and registered.

## The two implementations

### `ContactsAcceptPolicy` (daemon)

```rust
s.known_vault || self.contacts.knows(s.sender)
```

Gates on **sender** — "do I know who's pushing?" — against the daemon's
`contacts` table (resolving `did:web` entries to keys as needed). The
recipient is trivially the daemon itself, so it is not checked.

### `HubAcceptPolicy` (hub)

```rust
self.user_peers.controls(s.sender) && self.user_peers.hosts(s.recipient)
// …and on accept, record the `recipient → vault` hosting row.
```

Gates on **both ends**, against the hub's `user_peers` registry (a different
"peer book" entirely — multi-user, `kind`-tagged, the source of `did:web`
resolution and JWT auth; *not* a `PeerStore`):

- **recipient ∈ hosted devices** — the push is destined for a peer the hub
  relays for.
- **sender ∈ controlled devices** — the pusher is a device enrolled to the
  hub.

This is what lets cross-user, *same-hub* sharing work (Alice's enrolled daemon
→ Bob's enrolled browser — both controlled), while rejecting a foreign
internet peer trying to push junk to a hosted browser. Cross-*hub* pushes are
rejected; that would need federation, later. Integrity of accepted ciphertext
comes from the manifest signature (a shareholder), which the hub can verify —
manifests are signed, not encrypted.

In the design the hub's `accept_sync` is **side-effecting**: on `true` it
would record the `recipient → vault` relationship. **Deferred:** the current
`HubAcceptPolicy` performs the gate (`controls(sender) && hosts(recipient)`)
but does not yet persist that registration — there's no read-gate consumer for
it yet (the hub serves ciphertext to authenticated users; only shareholders
can decrypt). When per-vault read gating lands, the registration write hangs
off this same accept.

## Wire change

`recipient` must travel with the push:

```rust
HeadAdvanced { vault_id, head, recipient: PublicKey }
```

`announce_head` already iterates shareholders; for each share it knows the
client `C` and dials `C.via.unwrap_or(C)`. It stamps `recipient = C`:

- **direct dial** → `recipient == target` (redundant, harmless).
- **`via=hub` dial** → target is the hub, `recipient` is the browser key.

`recipient` is non-`Option`: every announce serves a specific shareholder, so
there is always a concrete recipient.

## Share population is daemon orchestration

Granting shares is not a protocol concern. The daemon reads its address book,
resolves DIDs, mutates the vault, and announces:

```
contacts.list_trusted()  →  resolve_reaches(did)  →  vault.add_reach()
                         →  vault.save()           →  peer.announce_head()
```

`reconcile_trusted` therefore lives in the **daemon** (the `zim` crate),
built on `Peer`'s public surface, not on `Peer` itself. See
[access-model.md](./access-model.md) for the trusted-contact reconcile and
the `contacts` table.

### Two predicates over one table

Inbound acceptance and outbound sharing use *different* predicates over the
daemon's contacts:

| Direction | Question | Predicate |
|-----------|----------|-----------|
| inbound (accept) | "do I *know* this sender at all?" | any contact (trusted **or** not) |
| outbound (share) | "who do I auto-share with?" | `trusted` only |

So a contact you added to share *one* vault with (untrusted) is accepted when
they push back, but is **not** auto-folded into your other vaults. Keep them
two distinct queries; do not collapse them onto one flag.

## What leaves the sync layer

- **`PeerStore` trait + `MemoryPeerStore`** — deleted. They existed only to
  feed the `P` generic. Tests use a trivial `AcceptAll` policy.
- **`SqlitePeerStore`** — kept, but demoted to plain concrete daemon storage
  (the `contacts` table). No longer a coordinator generic.
- **The DID resolver** — leaves the coordinator/`Peer` for `ServiceState`.
  Its only users (the old acceptance gate, the share handler, reconcile) are
  all daemon-side. `zim-peer` stops knowing about DID resolution.

## The boot seam

`ServiceState::boot` takes the policy as a parameter, so each binary supplies
its own:

```rust
// daemon
ServiceState::boot(home, ContactsAcceptPolicy::new(contacts, resolver))
// hub
ServiceState::boot_with_blobs(home, blobs, HubAcceptPolicy::new(db))
```

The hub no longer inherits the daemon's (empty) contacts book as its gate —
which today would make it reject every unknown vault, including legitimate
pushes from enrolled daemons. It gates on its own `user_peers` instead.

## Where it lives

| Concern | Code |
|---------|------|
| Hook + `IncomingSync` + `AcceptAll` | `zim-peer/src/accept.rs` |
| Coordinator (`SyncCoordinator<L>`, calls `accept_sync`) | `zim-peer/src/coordinator.rs` |
| `HeadAdvanced { …, recipient }`; `announce_head` stamping | `zim-peer/src/{messages,peer}.rs` |
| Daemon `ContactsAcceptPolicy` | `zim/src/accept.rs` |
| Daemon `reconcile_trusted` (generic over `L`) | `zim/src/reconcile.rs` |
| Resolver + policy wiring | `zim/src/service_state.rs` (`boot_with_blobs(home, blobs, accept)`) |
| Hub `HubAcceptPolicy` over `user_peers` | `zim-hub/src/accept.rs` (wired in `main.rs`) |

`PeerStore` survives only as the daemon's contacts-storage trait
(`SqlitePeerStore`); it is no longer a sync-protocol generic. `Effect::AnnounceHead`'s
`head` is boxed to keep the effect enum small now that it carries a `recipient`.

See also: [synchronization.md](./synchronization.md),
[access-model.md](./access-model.md), [identity.md](./identity.md).
