# Metadata privacy and the untrusted-mirror model

**Stage:** Design
**Priority:** Medium
**Dependency:** [Per-node identity and ratchets](per-node-identity-and-ratchets.md)

## The leak, precisely

Content is end-to-end encrypted (Cryptree per-entry keys). But the
**manifest is not encrypted** — the hub reads it with
`blobs.get_cbor::<Manifest>(link)`, no share. It is a cleartext, signed
CBOR index exposing `root, pins (the entire blob set), previous, height,
ops-link, shares (pubkeys + wrapped secrets), author, signature`. Only
what it *points at* — dir bodies, file content, the ops-log — is
encrypted.

So a single cleartext object is the entire structural-metadata leak, and
the encrypted DAG hangs off it.

## The binding principle

**The hub is not a privileged listener.** Anyone speaking the protocol
reads the same cleartext manifest and can index your blobs. This scopes
the hub-trust axiom correctly: that axiom is about **content**
confidentiality and **roster** integrity — it does *not* license
broadcasting your structure to the world. Metadata minimization is
legitimate defense against *any* listener, not theater aimed at a trusted
hub.

## The product decision that resolves the tension

A stated product requirement: **long-lived, untrusted peers should
swallow your updates**, so users get high availability without running
their own nodes. That decisively resolves what would otherwise be a
trilemma —

> Pick two of: (1) shareless zero-knowledge mirroring · (2) no privileged
> relays (hub = any listener) · (3) blob/structure privacy.

The product wants **(1) + (2)**, and therefore **accepts the structural
consequences of (3)**: open untrusted infra can only host your ciphertext
if it sees enough to do the job. This means the "encrypt everything /
make the hub dumb for privacy" direction is **off the table** — it fights
the product goal. Do not pursue it.

## What still survives: routing ≠ membership

The `Share` bundles three jobs, all keyed on cleartext DIDs:
`secret_share` (read-key distribution), `identity` (write-auth
membership), and `identity + via` (the **sync routing table**). The
product needs **routing**; it does **not** need cleartext **membership**.
They separate cleanly:

- **Push-by-DID (today):** the manifest is a cleartext address book — a
  mirror reads the shareholder roster + `via` to fan out. Leaks
  membership *permanently*, in every manifest.
- **Subscribe-by-vault-id (direction):** a mirror holds vault X's
  ciphertext and relays "X advanced to Y" to whoever is *currently
  subscribed to X*. It routes by **vault-id**, not identity. Membership
  stays encrypted; shareholders read it only to route *direct*
  daemon-to-daemon dials among themselves.

Subscribe-by-vault-id delivers the product feature while the mirror
learns only "someone cares about X," not *who your shareholders are*. It
also **improves the browser path**: a browser (no P2P transport, so the
hub is structurally its router) writes → mirror stores → subscribed
daemons pull. The mirror fans out to *subscribers*, so it no longer needs
the daemon DID routing table it uses today; `via`/`identity` can move
inside encryption, used only for direct dials.

## Write-auth without cleartext shares: write key + auth stub

Replace "author ∈ {cleartext pubkey list}" with a **per-vault write
keypair**. Shareholders (writers) hold the private half; every manifest
is signed by it; a verifier checks against the vault's write *public*
key. This gatekeeps writes while learning nothing about who or how many
shareholders exist. It matches current semantics (every shareholder can
write) and enables the read-only vs read-write share split.

**Do not make the vault id the write pubkey.** The id must be stable; the
write key *wants to rotate* (revocation mints a fresh one), so id =
write-pubkey would rename the vault on every revocation. `VaultId =
blake3(genesis)` is already stable and self-certifying — keep it.
Instead: **genesis embeds the initial write key; rotation is a delegation
chain** signed forward from it. A verifier checks "signed by a write key
reachable from the vault's genesis key," needing no per-vault state
beyond the genesis the id already commits to.

This lets a verifier stay useful without a cleartext manifest, via a
tiny **cleartext auth stub** per manifest —
`{ write-pubkey/delegation-ptr, previous, height, signature }` — with
everything else (root-secret pointer, shares, structure) encrypted. The
stub is what makes **open untrusted mirrors safe**: an open mirror
accepting "vault X advanced to Y" from anyone is a spam/forgery magnet;
the stub lets a content-blind mirror **reject unauthorized head
advances** by verifying the signature, without reading a byte. The
product requirement (untrusted mirrors) is what motivates the stub, and
the stub is what makes untrusted mirrors safe.

Read membership can also be de-indexed: store the wrapped secrets
**unindexed** (trial decryption) so a listener sees N opaque blobs, not
whose.

## Unlinkability via key derivation (application layer only)

A device that reuses one stable pubkey as its share identity across
vaults A, B, C lets any observer correlate those vaults to one identity.
Fix at the application layer with **per-vault derived keys**:
`pk_vault_i = KDF(device_master, vault_id_i)`. Manifests and mirror
subscriptions then show *unrelated* pubkeys across your vaults.

Two boundaries, stated honestly:

- **Transport layer is not covered.** iroh addresses by NodeId
  (= pubkey), so being reachable as `pk_vault_i` means advertising
  multiple identities — but if they resolve to the same relay/IP, an
  observer (or the relay operator) re-correlates at the network layer.
  True transport unlinkability is the Tor/mix-network problem; "advertise
  multiple pubkeys" is necessary but not sufficient. Scope the win as
  **unlinkable membership, not unlinkable network presence**. It matters
  most on the *mirror* path (blind untrusted infra); on *direct dials*
  between existing collaborators, correlation is far less sensitive.
- **Collides with the did:web account model.** An account is a stable,
  discoverable `did:web` with a device roster — that stability is how
  people share *to you*. Per-vault derived keys want the opposite. Key
  derivation is clean for **daemon peers**; for the **account/browser**
  side it forces a product call: stable public identity (shareable-to but
  linkable) vs unlinkable per-vault keys (private but harder to share to).

## Layered end-state

Given the product decision, the coherent target:

- **Leaked (accepted):** vault existence, blob count/sizes, update
  cadence — the price of open untrusted availability.
- **Protected:** content, shareholder membership (encrypted +
  subscribe-by-vault-id delivery), tree structure, history.
- **Mechanism:** genesis-hash id → genesis-embedded write key +
  delegation chain → cleartext auth stub the mirror verifies → encrypted
  body (read secret, split read/write shares, routing-for-direct-dial) →
  mirrors relay by vault-id to subscribers → per-vault derived keys blind
  the mirror path.

## Open questions

- Subscribe-by-vault-id trades a permanent roster leak for
  traffic-analysis leakage (which connections care about which vault-ids,
  and update timing). Better for most threat models, but not nothing.
- Subscription auth: open ("anyone may watch X's ciphertext stream") vs
  proof-of-share. Open avoids identity leak but exposes update cadence to
  anyone.
- Delegation-chain format and rotation UX (revoke a device → rotate write
  key → extend the chain).
- Anti-spam for open mirrors beyond the auth stub (storage exhaustion
  from authenticated-but-garbage heads).
