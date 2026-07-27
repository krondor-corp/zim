# Relay vs. mirror vs. share — access taxonomy

**Status:** design note. **Not implemented — deferred.** Captures a rethink of
what "relay" means and how it differs from a pure "mirror," plus how hub
mirroring should be gated. We are not building this yet; this is a marker so
the distinction isn't lost.

> **Update — converged on "hosted DID."** The relay/share split below was
> superseded by a cleaner framing: a relay is simply **a share whose identity
> is a hosted `did:web`**, where the DID's host *is* the via. There is no
> separate `Relay` struct or `via` field — the DID method carries it, and
> resolution returns the full relay form (client key + via key). See
> **`docs/product/identity.md` → "DID forms (identities)"** for the canonical
> model. The "mirror" (sync-only, no recipient) and the device-enrollment gate
> below are still the open/deferred pieces.

## The three concepts

There are **three** distinct things, not two. The current code only models the
first two (and conflates the hub-as-mirror case into a relay).

| Concept    | Shape                      | Meaning                                                            |
|------------|----------------------------|-------------------------------------------------------------------|
| **Share**  | `{ identity, secret_share }` | Who can **decrypt** — holds the vault secret sealed to their key. |
| **Relay**  | `{ via, recipient }`       | **Share** to `recipient`, served through always-on `via`. Implies granting access. |
| **Mirror** | *"just sync this"*         | Pure availability — hold/serve ciphertext. **No recipient, no sharing.** |

## Relay is already shaped right

`zim_core::fs::Relay` (`crates/zim-core/src/fs/share.rs`) already encodes the
rethought model:

- `via` — *the peer that is listening* (the always-on intermediary, e.g. the hub).
- `recipient` — *who we're sharing it with* through that via (e.g. a browser
  session key).

So a relay **implies sharing**: it's "serve this vault to `recipient`, routed
through `via`." A relay is a sharing edge with an intermediary — **not** a dumb
mirror.

## Mirror is the missing concept

A pure **"just sync this"** is a true mirror: no `recipient`, no access grant —
just "hold and serve these ciphertext bytes." It is **not** currently modeled as
its own thing. Today the hub-holds-your-own-vault case is shoehorned into a
`Relay` via the `zim vault <id> relays add hub` flow (which sets
`recipient = browser, via = hub`).

Per the corrected model that's wrong: the hub holding **your own** vault for
**your own** shareholders is a **mirror**, not a relay. A relay is specifically
when you grant **some other** `recipient` access via an intermediary.

Consequence: the boot message in `crates/zim-hub/src/main.rs:70-72`
(`zim vault <vault-id> relays add hub`) is really asking for a **mirror**,
mislabeled as a relay.

## Gating: hub mirroring is restricted to a user's devices

The hub must not mirror arbitrary ciphertext. **Mirroring is gated: the hub only
mirrors a vault when the pushing peer is a device under a user** — i.e. enrolled
in the hub's `user_peers` table. Gate on **enrollment / ownership** (the peer is
a device belonging to some user), **not** on live/online status.

This is the hub-side complement of the daemon-side "address book = auto-accept"
gate: the hub's address book *is* its set of enrolled devices.

## Open questions (deferred)

- **How to represent a mirror?** A distinct manifest field (e.g.
  `mirrors: Vec<Identity>` — always-on peers asked to hold ciphertext, no
  recipient), or purely hub-side state (the hub tracks "I mirror vault X for
  user U" in its DB, nothing in the manifest)?
- **Migrate `relays add hub`** to a `mirror`-flavored command/flow once mirror
  exists, and fold hub-mirroring into login/address-book defaults so the manual
  per-vault step disappears.

## Related

- `docs/research/optimistic-share-acceptance.md` — tentative log entries for
  non-blocking bootstrap.
- The in-flight DID-aware `PeerStore::knows()` gate (daemon side) — the
  acceptance mechanism this mirroring gate mirrors on the hub side.
