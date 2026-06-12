# WNFS Takeaways — HAMT, Skip Ratchets, Shares-as-Pointers

**Date:** 2026-06-03
**Scope:** What zim should learn from WNFS's private filesystem
(`wnfs-wg/spec`, `wnfs-wg/rs-wnfs`) for three specific problems:
scaling shares / dir bodies / pin sets, cheaper key rotation, and
representing shares as references into shared structure rather than
copied blobs.
**Reading order:** §0 → §1 → §2 → §3 → §4. §5 is the phased adoption
plan; §6 catalogs things zim should *not* copy.

## §0 Executive summary

**zim and WNFS share the adversarial-storage threat model.** Relays,
iroh peers, and hubs are all untrusted storage that holds ciphertext
on behalf of vault owners. AEAD handles content confidentiality at
the blob layer, but *structural metadata* — how many blobs a vault
has, which blobs reference which, write/read patterns over time — is
a separate axis where zim has open trade-offs that WNFS spent
significant engineering on.

The honest picture:

- **Today**: zim writes one big encrypted manifest blob per
  revision. Relays see one ciphertext per save. Zero structural
  leak from manifest shape; full content opacity from AEAD.
- **With HAMT** (proposed in this doc): zim writes N smaller blobs
  per revision. Relays still can't decrypt, but can now observe
  blob count, blob sizes, fetch ordering (correlated with subtree
  walks), and write timing (correlated with which subtree was
  mutated). A real privacy regression vs single-blob today, in
  exchange for O(log N) scaling instead of O(N).
- **With WNFS's full accumulator scheme**: structural metadata also
  hidden — labels are unlinkable across siblings and revisions,
  cardinality is hidden, depth is hidden. Expensive (RSA-2048 +
  hashToPrime) and a large code surface.

What's worth porting:

1. **A 16-way Merklized HAMT** for scale. Open question on label
   scheme — see §1 for the trade-off between raw Blake3 (cheap, no
   structural privacy at the HAMT layer), HMAC-keyed (cheap,
   blocks cross-vault correlation), and accumulators (expensive,
   full unlinkability).
2. **Skip ratchets** (`skip_ratchet` crate) for cheaper key
   rotation, stable sub-keys at different cadences, and 96-byte
   "from-epoch-T-forward" temporal grants.
3. **Shares as pointers** (`PrivateRef`/`AccessKey`) instead of
   inline wrapped keys. Grants become small encrypted pointers into
   shared structure; revocation becomes ratchet advance + re-issue.

And one specifically-relay use case for **DIDs + UCANs** that
WNFS's design didn't surface but matches zim's relay/hub model
exactly (§6).

The three primitives compose. The leaf shape, key rotation cadence,
share representation, and relay grants all become cleaner once you
adopt them together, with explicit decisions about the
structural-privacy trade-off.

## §1 The HAMT (`wnfs-hamt`)

### Structure

```rust
pub struct Node<K, V, H = blake3::Hasher> {
    persisted_as: OnceCell<Cid>,
    pub(crate) bitmask: BitArray<[u8; 2]>, // 16-bit occupancy mask
    pub(crate) pointers: Vec<Pointer<K, V, H>>,
    hasher: PhantomData<H>,
}

pub enum Pointer<K, V, H> {
    Values(Vec<Pair<K, V>>),   // up to HAMT_VALUES_BUCKET_SIZE
    Link(Link<Arc<Node<...>>>) // child node when bucket overflows
}
```

- Fanout: **16** (4-bit nibble per level).
- Bitmask: 16-bit, stored as `[u8; 2]`. Only occupied slots appear
  in `pointers`, indexed by `popcount(bitmask & ((1 << bit) - 1))`.
- A `Values` bucket holds direct key-value pairs until it overflows;
  on overflow, all pairs (plus the new one) are rehashed into a
  freshly-allocated child `Node` and the slot is replaced with
  `Pointer::Link`. This is the "sharding" mechanism — the trie
  deepens lazily only where collisions actually happen.
- History-independence: insertion order doesn't affect the resulting
  CID. Property-tested in
  `node_operations_are_history_independent`. **This is the property
  that makes three-way merges trivial** — divergent peers that
  inserted the same set in different orders converge to the same
  node hash.

### Serialization

```rust
NodeSerializable(ByteArray<2>, Vec<PointerSerializable>)
```

CBOR-encoded sparse representation. Empty slots cost zero bytes.
Each `Pointer::Link` resolves to a child CID before storage, so a
HAMT is a Merkle DAG and partial updates dirty only the path from
root to leaf (≈ log₁₆ N blobs).

### What to port into zim

- The trie shape, the bitmask + popcount lookup, the bucket-split
  logic, the CBOR serialization. ≈ 600 lines of code based on the
  rs-wnfs implementation size.
- Use as the backing structure for:
  - **`Manifest::shares`** (today a `BTreeMap<PublicKey, Share>`
    rebuilt on every grant/revoke)
  - **Directory bodies** (today flat encoded entry lists that get
    fully rewritten on any mutation)
  - **Chunk index** for large files (post-v3 chunking work)
  - **Pin sets** on the peer (today a flat tracking structure)
  - **Relay grants** (§6)
- API shape:
  ```rust
  trait Tree<K, V> {
      async fn get(&self, k: &K) -> Result<Option<V>>;
      async fn insert(&mut self, k: K, v: V) -> Result<()>;
      async fn remove(&mut self, k: &K) -> Result<Option<V>>;
      async fn merge(&mut self, other: &Self) -> Result<()>;
  }
  ```
  Existing `Manifest`/dir body code becomes a thin wrapper over
  `Tree<...>`.

### Label scheme — three options on a privacy/cost curve

Relays in zim's model are adversarial storage. The label scheme
determines what they can infer about vault structure when the HAMT
is serialized to blobs they hold. Three options, increasing cost:

**Option A — raw `blake3(path)` labels.** Cheapest. Inside-ciphertext
today, but if any layer ever surfaces HAMT-internal addressing
(e.g. iroh collection links that expose child references in
cleartext, or a future debug/inspection feature), an adversary
holding a candidate path can precompute its label and check for
existence. Cross-vault correlation also works — same path in two
vaults produces the same label. **Use only if you're certain HAMT
internals stay inside AEAD and cross-vault correlation is
acceptable.**

**Option B — HMAC-keyed labels: `blake3_keyed(label_key, path)`.**
The `label_key` derives from the skip-ratchet's stable sub-key
(same one used for plaintext fingerprints — §2). Adversary can't
precompute labels without vault membership; cross-vault correlation
is blocked (each vault's labels live in a private namespace). Same
asymptotic cost as Option A (one Blake3 call); roughly 5 extra
lines of code. **Recommended default.** Pays for the case where
"HAMT internals never leak" turns out to be wrong, with effectively
zero overhead.

**Option C — full WNFS accumulators.** RSA-2048 modulus + prime
hashing per label. Hides sibling existence and depth from anyone
who can see the HAMT structure even in plaintext form. Required
only if zim ever exposes HAMT-internal addressing directly to
relays. ≈ 2000+ lines of additional crypto code, real CPU cost
per write. **Defer until a concrete threat materializes** — for
now, Option B is the right operating point.

### Residual structural leak

Even with Option B labels, the HAMT-vs-single-blob switch leaks
*access-pattern* information to relays that no label scheme fixes:

- Blob count (rough vault-size proxy)
- Blob-size distribution (internal nodes small, leaf values
  variable)
- Fetch ordering (consecutive blob fetches likely sibling/parent
  in trie walks)
- Write timing (which set of blobs change in close temporal
  proximity → which subtree was mutated)

This is a real trade-off vs the single-big-blob shape today.
Mitigations exist (padded writes, dummy fetches, mixnet-style
relay protocols) but they're a separate axis from the data
structure choice. Document the trade-off; don't pretend it's not
there.

### Other trade-offs

- Adds an extra blob fetch per traversal level on cold reads. For
  small structures (< ~100 entries) the flat shape is actually
  faster — single blob, no traversal. The win shows up at scale.
  Don't HAMT a vault with 12 shares; do HAMT one with 12,000.
- Sub-entry-size updates still rewrite the leaf blob; HAMT only
  amortizes structural cost.

## §2 Skip ratchets (`skip_ratchet` crate)

### Mechanism

96 bytes of state:

```
salt:        [u8; 32]
large:       [u8; 32]   // slowest counter — epoch
medium:      [u8; 32]
medium_count: u8
small:       [u8; 32]
small_count:  u8
```

A base-256-cubed counter (≈16M positions per `large` epoch). Each
"step" hashes the appropriate state through Blake3. Key derivation:

```rust
key = blake3::derive_key(domain, large || medium || small)
```

- Advance by 1 within `small`: 1 hash, **O(1)**.
- Carry over `small_count = 255` → bump `medium`: 3 hashes, **O(1)**.
- Carry over `medium_count = 255` → bump `large`: a few more hashes,
  still **O(1)**.
- Backward derivation: infeasible (pure hash chain, no inverse).

Method surface in rs-wnfs:

```rust
pub struct PrivateNodeHeader {
    inumber: NameSegment,
    ratchet: Ratchet,
    name: Name,
}

impl PrivateNodeHeader {
    pub fn advance_ratchet(&mut self) { self.ratchet.inc(); }
    pub fn derive_temporal_key(&self) -> TemporalKey {
        TemporalKey::new(&self.ratchet)
    }
}
```

The ratchet position **is** the revision id; the temporal key is
derived directly from it.

### What this gives zim

Today zim conceptually rotates a content key on every write. That
means either (a) the new key is random and you must wrap it under
every share, or (b) the new key derives from a stable secret —
which we don't have because rotation kills stable secrets.

Skip ratchet replaces the stable-secret-or-random dichotomy:

1. **Vault state becomes a ratchet position.** "Current write key"
   = `derive_key("zim/content/v1", ratchet)`. Advancing the ratchet
   is advancing one position; the new key is deterministic from the
   new position.
2. **Stable sub-keys for free.** Multiple derivation domains off the
   same ratchet position let you have different rotation cadences:
   ```
   write_key       = derive_key("zim/content/v1", ratchet@N)
   fingerprint_key = derive_key("zim/fingerprint/v1", ratchet@floor(N/EPOCH))
   ```
   The fingerprint key only changes when you cross epoch boundaries
   — which can be aligned with revocation events. This is the missing
   piece for the plaintext-fingerprint-under-rotation problem from
   the earlier sync-optimization discussion.
3. **Temporal access grants are a 96-byte handoff.** Hand someone a
   `Ratchet@T` snapshot; they can advance to T+1, T+2, … forever, but
   cannot derive T-1. "Bob can read from height 1000 forward" is
   trivially representable.
4. **Revocation = advance `large`.** All retained sub-`large`
   positions are now in a stale epoch. Non-revoked shares get the
   new `large` state out-of-band; revoked ones can't catch up.

### Caveats

- The "skip ahead N positions in true O(1)" claim relies on a 2022
  IACR paper. The rs-wnfs implementation appears to support O(log N)
  catch-up via `RatchetSeeker`/`JumpSize`, which is plenty for
  practical peer-online-after-being-offline scenarios.
- Forward secrecy is *within an epoch*. An attacker who steals
  `small@50/256` can iterate forward to read 51..255 of that
  medium-epoch but cannot derive earlier `small` states from the
  same epoch (they would have to invert Blake3). Good enough.
- The crate is BSD-2-Clause licensed, looks low-dependency, and the
  rs-wnfs project depends on `skip_ratchet = "0.3"`. Worth a one-day
  evaluation before adopting blindly.

## §3 Shares as pointers (`PrivateRef`, `AccessKey`)

### The pointer

```rust
pub struct PrivateRef {
    label: HashOutput,           // Blake3 hash → HAMT lookup key
    temporal_key: TemporalKey,   // 32 bytes, derived from ratchet
    content_cid: Cid,            // disambiguates multivalue
}
```

A `PrivateRef` is **all you need to read a node**:
- `label` finds the entry in the shared HAMT (the "forest")
- `temporal_key` decrypts what you find there
- `content_cid` picks the right value if the HAMT slot has multiple
  conflicting writes (concurrent writes from different peers)

### The wrapping

```rust
pub enum AccessKey {
    Temporal(TemporalAccessKey),  // read-current-and-future
    Snapshot(SnapshotAccessKey),  // read-this-revision-only
}
```

A `TemporalAccessKey` is `{label, content_cid, temporal_key}` — a
`PrivateRef` plus a domain tag. A `SnapshotAccessKey` swaps in a
`SnapshotKey` derived from the temporal key, narrowing access to one
revision.

### The grant flow

```rust
pub async fn share<K: ExchangeKey>(
    access_key: &AccessKey,
    share_count: u64,
    sharer_root_did: &str,
    recipient_exchange_root: PublicLink,
    forest: &mut impl PrivateForest,
    store: &impl BlockStore,
) -> Result<()>
```

What actually happens:
1. Sharer fetches the recipient's exchange public key from an
   "exchange root" (a published, signed key bundle).
2. The `AccessKey` is **serialized and encrypted under the
   recipient's public key** (asymmetric, RSA in WNFS — would be
   X25519 in zim).
3. The encrypted blob is stored in the *sharer's* private forest
   under a label derived from `(sharer_did, recipient_pubkey,
   counter)`.

The recipient retrieves via:
```rust
pub async fn receive_share(
    share_label: &Name,
    recipient_key: &impl PrivateKey,
    forest: &impl PrivateForest,
    store: &impl BlockStore,
) -> Result<PrivateNode>
```

They compute the same composite label, fetch the encrypted blob,
decrypt it with their private key, get an `AccessKey`, then use it
to read the actual data node.

### Why this matters for zim

Today zim's shares are inline records on the vault manifest (a
public key + a wrapped key per share). The grant operation rewrites
the shares list. Two consequences:

1. **The shares list scales O(N) per grant/revoke** — covered by
   §1 HAMT.
2. **Every share carries its own copy of vault metadata** because
   the wrapped key in the share is what gates access to the
   content. Granting Bob access doesn't reference shared state; it
   re-wraps state for Bob.

The WNFS shape inverts that. The "shared state" is the private
forest (the HAMT of encrypted nodes). A share is a *pointer* into
it — a small encrypted blob containing `{label, key, content_cid}`
that lets the recipient look up and decrypt nodes that already
exist.

For zim, the corresponding shape is:

```rust
// Vault-level: an HAMT of revisions / file metadata, encrypted
// under ratchet-derived keys. All shares reference the same trie.

// Per-share grant:
struct ShareGrant {
    recipient_pubkey: X25519PublicKey,
    encrypted_access_key: Vec<u8>,  // X25519-encrypted AccessKey
}

// What the recipient gets after decryption:
struct AccessKey {
    forest_root: Link,           // CID of the HAMT root
    label_seed: HashOutput,      // entry point into the HAMT
    ratchet: Ratchet,            // position in skip ratchet → keys
    granted_at_height: u64,      // for audit/temporal scope
}
```

- Granting Bob: derive an `AccessKey` for the subtree he should see,
  X25519-encrypt to Bob's pubkey, store the ciphertext.
- Revoking Bob: advance the ratchet's `large` counter, re-issue
  `AccessKey`s for non-revoked members. Bob's existing `AccessKey`
  is now positioned in a stale epoch.
- Snapshot vs temporal: derive a `SnapshotKey` from the ratchet
  position rather than handing over the ratchet state. The
  recipient can decrypt that revision but cannot advance.

The cost of a grant collapses from "rewrite the shares list and
re-wrap" to "produce one X25519 ciphertext and put it in the HAMT."

## §4 Putting the three together

The three primitives are not independent — they compose into a
coherent shape:

```
Vault state:
  - Ratchet position (96 bytes), advances per write
  - Forest root (CID): HAMT of encrypted PrivateNodes
  - Shares HAMT (CID): HAMT of {recipient_pubkey → encrypted AccessKey}

Write path:
  1. Read-modify-write the affected dir bodies (HAMT updates)
  2. Advance ratchet
  3. Derive new write_key from ratchet@N+1
  4. Encrypt and store new node bodies under write_key
  5. Update HAMT entries
  6. New forest root CID lands in the bucket log

Read path (existing share):
  1. Decrypt AccessKey with own X25519 secret
  2. Walk forest HAMT to {label}
  3. Catch up ratchet position to current head (RatchetSeeker)
  4. Derive temporal_key for current revision
  5. Decrypt node body

Grant:
  1. Form AccessKey {forest_root, label, ratchet@now}
  2. X25519-encrypt to recipient pubkey
  3. Insert into shares HAMT keyed by recipient pubkey

Revoke:
  1. Advance ratchet by one large-epoch step
  2. Remove the revoked entry from shares HAMT
  3. Re-issue AccessKeys to non-revoked shares (new ratchet state)
```

Sync optimization (from the earlier discussion) drops in cleanly:

- Each `Leaf::File` in a dir body's HAMT carries `plaintext_hash`
  (the v1 work the other agent is implementing today).
- v2's chunk index per large file is itself a HAMT, with
  per-chunk `(link, plaintext_hash)` entries.
- v2's chunk encryption uses the *stable* sub-key — derived from
  `ratchet@floor(N/EPOCH)` rather than `ratchet@N` — so convergent
  encryption actually works between rotations.

## §5 Phased adoption

The dependency order is real — later phases assume earlier ones.

1. **v1 — plaintext_hash on Leaf::File.** Already in progress.
   Pure schema addition, no new crypto. Ships independent sync
   wins for the "most files unchanged" case.
2. **v1.5 — skip ratchet.** Replace per-write random key
   generation with ratchet derivation. Single largest crypto
   change. ≈ 300 lines including audit + integration tests.
   Unblocks stable sub-keys, temporal grants, cheap revocation.
3. **v2 — HAMT for shares + dir bodies.** Generic
   `wnfs-hamt`-style implementation, Blake3 labels, no
   accumulators. ≈ 600 lines plus migration code.
4. **v2.5 — AccessKey-style shares (shares-as-pointers).** Move
   the shares structure to `(recipient → encrypted AccessKey)`
   pairs. Smaller than it sounds because the HAMT and ratchet are
   already in place; this is mostly the wrapping/unwrapping flow
   plus the grant/revoke ops.
5. **v3 — FastCDC + convergent encryption per chunk** (the other
   agent's v2). Built on the stable sub-key from v1.5 and the
   chunk-index HAMT from v2. FUSE sparse-read layer falls out.

Each phase is independently shippable: v1 alone improves sync
today, v1.5 alone enables cheap rotation, v2 alone makes large
share lists practical. The composability is the point — none
of these is forced upon the others.

## §6 Where DIDs and UCANs *do* earn their keep: relays and hubs

The blanket "no DID/UCAN" was too strong. For **peer-to-peer share
grants** they're over-engineered. For **relays and hubs** — services
that hold ciphertext on behalf of vault owners and serve it to
others — they're the right shape.

### What a relay actually is in zim

A relay is a peer that:
- Stores encrypted blobs on behalf of a vault owner
- Serves them to other peers on request
- Cannot decrypt anything (no vault share)
- Is operationally long-lived (always-on hosting infrastructure)

It's a **very limited share**: storage and serve permission, no
read access, attributable to a specific peer (or service) identity.
Today zim models this as `Relay::new(peer_pubkey)` — a bare pubkey
with implicit semantics. That works but leaves three things on the
table that DIDs+UCANs solve directly.

### What DIDs buy specifically for relays

1. **Key rotation portability.** An always-on hub may legitimately
   need to rotate its operational signing key (compromised, hardware
   replaced, planned hygiene). A bare-pubkey grant dies on rotation;
   every vault that authorized that hub has to re-issue. A
   `did:web:hub.example.com` grant survives — the hub republishes
   its DID document with a new pubkey, and the grant references the
   stable DID. **This is the only DID property with real
   architectural weight for zim.**

2. **Discoverability through a well-known resource.** A hub
   advertises itself at a predictable URL:
   ```
   GET https://hub.example.com/.well-known/zim-hub
   →
   {
     "did": "did:web:hub.example.com",
     "pubkeys": {
       "ed25519": "abc...",            // current signing key
       "ed25519_previous": ["xyz..."]  // for grace-period verification
     },
     "endpoints": {
       "iroh": "iroh://...",
       "http": "https://hub.example.com/api"
     },
     "policies": {
       "max_blob_bytes": 10_737_418_240,
       "retention_days": 30
     }
   }
   ```
   This is just the `did:web` DID-document shape. Any peer can
   resolve a hub's DID to its current operational identity without
   trusting a registry. A vault owner adding a hub by URL gets
   automatic key-rotation tolerance.

3. **Human-readable identity in grants.** `did:web:hub.quotient.ai`
   is something users can read and reason about; `f3b27a...` is not.

### What UCAN-shaped capabilities buy specifically for relays

A relay grant is exactly a UCAN: a signed, narrowly-scoped,
attenuable, optionally-time-bound capability handed from one
identity to another.

```rust
struct RelayGrant {
    /// Vault owner's pubkey or DID. The "issuer".
    issuer: Identity,
    /// Hub's DID (typically did:web). The "audience".
    audience: Identity,
    /// Which vault this grant covers.
    vault_id: Uuid,
    /// Narrow capability — relays cannot grant *read* via this.
    capabilities: RelayCapabilities,
    /// Optional sub-delegation: if Some, the audience may issue
    /// further-attenuated grants on this vault. Most grants set
    /// this to false.
    delegable: bool,
    /// When the grant was issued (ratchet position, not wall-clock).
    issued_at: u64,
    /// Optional ratchet position past which the grant expires.
    valid_until: Option<u64>,
    /// Ed25519 signature by `issuer` over everything above.
    signature: Signature,
}

enum Identity {
    /// Raw key — same as did:key, just typed.
    Key(Ed25519PublicKey),
    /// Resolves at use time. Carries cached fingerprint to detect
    /// key swaps that don't match the previous_pubkeys list.
    Web { url: String, fingerprint_hint: Option<HashOutput> },
}

struct RelayCapabilities {
    can_store: bool,                  // accept PUT blob
    can_serve: bool,                  // serve GET blob
    max_blob_bytes: Option<u64>,
    max_total_bytes: Option<u64>,
}
```

The hub presents the grant when peers ask "are you authorized to
serve vault X?" Peers verify by:
1. Resolving the `issuer` DID to the vault owner's pubkey (which
   they already know — they have the vault).
2. Checking the signature.
3. Confirming `audience` matches the hub they're talking to.
4. Optionally checking `valid_until` against current head ratchet
   position.

For sub-delegation (one hub forwards to another for load-spreading
or geo-locality): the secondary hub presents its own grant from the
primary, plus the primary's grant from the vault owner. Verifier
walks the chain back to the vault owner's signature. **This is the
UCAN delegation chain, applied to the one place in zim where multi-
hop trust actually shows up** — relay/hub federation.

### Storage of relay grants

Two natural shapes, complementary:

- **In the vault manifest** (under the HAMT, naturally): the
  vault's own list of authorized relays. Anyone with vault access
  knows which hubs the owner has blessed. Discovered out-of-band by
  the owner adding a hub URL; recorded in the manifest.
- **Advertised by the hub at its well-known endpoint**: the hub
  publishes its current set of "vaults I serve" by exposing the
  grants it holds. A peer with a vault id can query a hub's
  well-known endpoint to ask "do you serve this?" and get back the
  grant for verification.

### Why this composition makes sense

The four primitives stack cleanly:
- **HAMT** holds the relay-grants list at scale.
- **Skip ratchet** gives `valid_until` ratchet-position semantics
  (deterministic from vault state, not wall-clock-dependent).
- **AccessKey-style share** is the right shape for *read* grants
  to other peers; the **RelayGrant** is the parallel shape for
  *storage* grants to hub infrastructure.
- **DID + signed capability** is the right shape exactly when a
  grant crosses the human/service boundary, where key rotation
  and discoverability matter. Direct peer-to-peer shares don't
  cross that boundary; relays and hubs do.

### Scope discipline

Don't drag the whole UCAN spec in. Specifically:
- **JWT envelope** — skip it. Use CBOR or typed Rust structs.
- **Generic caveat DSL** — skip it. The `RelayCapabilities` struct
  with explicit fields is plenty; add fields as needs surface.
- **Resource URI scheme** — skip it. Vault ids are native types.
- **DID method registry** — implement only `did:key` (trivial) and
  `did:web` (one HTTP fetch). Skip `did:plc`, `did:ion`, etc.

What survives is small: a typed `RelayGrant` struct, an Ed25519
signature, an Identity enum that handles `did:key` and `did:web`,
and a `.well-known/zim-hub` endpoint on hubs. ≈ 400 lines.

## §7 Things to defer or skip from WNFS

- **RSA accumulators / `NameAccumulator`** (Option C labels in
  §1). The cardinality-hiding property is real and the threat
  model overlap with zim is real — but the cost is steep (RSA-2048,
  hashToPrime, large surface area). At zim's current layering HAMT
  internals stay inside AEAD-encrypted blob bodies, so the property
  is overkill *today*. Defer; revisit if zim ever exposes HAMT
  structure directly to relays, or if the access-pattern leak
  from §1's "residual structural leak" subsection turns out to
  matter operationally.
- **Public/exchange/private partition split.** WNFS has both a
  public (unencrypted) tree and a private (encrypted forest). zim
  is all-private; the split is unnecessary.
- **DIDs as core peer identity.** zim peers are Ed25519 pubkeys.
  Wrapping every peer in a DID layer adds resolution overhead and
  type ceremony for no win — peers don't rotate operational keys
  the way hub services do. *Use DIDs only at the relay/hub
  boundary* (§6), not as core peer identity.
- **`Name` / `NameSegment` plumbing.** Designed around the
  accumulator scheme. With Option B labels (HMAC-keyed Blake3),
  a name is just a path string and the HAMT label is
  `blake3_keyed(label_key, path)`. Much simpler than `Name`.
- **Full UCAN spec.** Adopt the *shape* (signed, attenuable,
  delegable, audience-targeted capabilities) for relay grants in
  §6; skip the JWT envelopes, generic caveat DSL, and resource
  URI scheme. Direct peer-to-peer shares use the simpler
  AccessKey-only model from §3.

What's left after subtracting these is small and clean: a Blake3
HAMT with HMAC-keyed labels, a skip ratchet, an X25519-wrapped
`AccessKey` shape for peer shares, and a UCAN-shaped `RelayGrant`
for hub/relay infrastructure. That's the durable, portable core
of WNFS's design plus the relay-specific extensions. The
structural-privacy upgrade (Option C accumulators) is a known
future option, not a foreclosed one.

## §8 References

- Spec: `https://github.com/wnfs-wg/spec/blob/main/spec/private-wnfs.md`
- Spec: `https://github.com/wnfs-wg/spec/blob/main/spec/skip-ratchet.md`
- Code: `https://github.com/wnfs-wg/rs-wnfs/tree/main/wnfs-hamt`
- Code: `https://github.com/wnfs-wg/rs-wnfs/blob/main/wnfs/src/private/node/header.rs`
- Code: `https://github.com/wnfs-wg/rs-wnfs/blob/main/wnfs/src/private/share.rs`
- Code: `https://github.com/wnfs-wg/rs-wnfs/blob/main/wnfs/src/private/keys/privateref.rs`
- Code: `https://github.com/wnfs-wg/rs-wnfs/blob/main/wnfs/src/private/keys/access.rs`
- Crate: `skip_ratchet = "0.3"` (used by rs-wnfs)
- IACR 2022/1078 — skip-ratchet paper (cited by spec, not read
  for this synthesis)
