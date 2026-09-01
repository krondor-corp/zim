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

## Shares are published — that is the delivery mechanism

The `Share` carries three things, all in the clear:
`secret_share` (the read secret wrapped to a recipient), `identity` (the
recipient DID), and `via` (the host it is reached through). All three are
**published in the manifest and must stay that way**, because that is how
delivery works:

- The hub fans out a browser-authored head to daemon shareholders along
  each share's `identity`/`via` — the browser has no P2P transport, so
  the hub is structurally its router.
- Untrusted mirrors relay updates using the same routing.

So under the product decision, **shareholder membership is visible** (the
recipient identities are right there in the routing), and the read
secrets are wrapped-but-indexed by recipient. Trying to hide the shares
(encrypt them, de-index them for trial decryption) is incoherent here —
it would break the exact delivery path the product depends on. We
**explored and rejected** it. The leak of *who shares a vault* is
accepted, same as blob count/sizes.

The one metadata lever that survives is **cross-vault unlinkability** —
see below — which does not hide membership *within* a vault, only stops a
listener correlating the *same device across* vaults.

## Per-vault write key — optional, and NOT a privacy device

Worth being clear, because an earlier draft over-sold this: a per-vault
**write keypair** (manifests signed by it; verifiers check the vault's
write *public* key) does **not** buy privacy. Membership is already
public via the published routing (above), so hiding it from *write-auth*
changes nothing an observer can't already see.

What a write key *does* buy, and the only reasons to consider it:

- **Forgery/spam protection for open mirrors.** An open untrusted mirror
  accepting "vault X advanced to Y" from anyone is a spam magnet. A
  published write *pubkey* lets a content-blind mirror reject
  unauthorized head-advances by verifying the signature — no share, no
  decryption. (Today's model already gets this via author-signature +
  author-in-shares; a single write key is a mild simplification —
  verify one pubkey instead of walking the shareholder set.)
- **Read-only vs read-write shares.** Sealing the write key only to
  writers gives a capability split the current all-or-nothing share
  can't express.

If adopted, two sub-decisions hold: **the vault id stays `blake3(genesis)`
— do not make it the write pubkey** (the write key rotates on
revocation; the id must not), and rotation is a **delegation chain**
signed forward from a genesis-embedded initial write key, so a verifier
needs no per-vault state beyond the genesis the id already commits to.

There is **no cleartext-auth-stub / encrypted-manifest** step — that
belonged to the encrypt-everything direction the product decision ruled
out. The manifest stays relay-readable.

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
  cadence, AND **shareholder membership + routing** — all the price of
  open untrusted availability. Shares are published; the manifest stays
  relay-readable.
- **Protected:** **content only** (Cryptree + per-entry ratchets), plus
  **cross-vault unlinkability** via per-vault derived key identities (a
  listener can't tell the same device is in multiple vaults — but sees
  each vault's membership).
- **Optional hardening:** a per-vault write key (forgery/spam rejection
  for open mirrors + read/write share split), id stays genesis-hash,
  rotation via genesis-rooted delegation. Not a privacy feature.

## Open questions

- **Cross-vault unlinkability vs the did:web account model** — per-vault
  derived keys want unlinkable identities; a `did:web` account wants a
  stable, discoverable roster (that's how people share *to* you). The
  account side may have to pick one. Clean for daemon peers.
- **Transport-layer correlation** — even with derived per-vault keys, a
  device reachable at multiple identities that resolve to the same
  relay/IP is re-correlated at the network layer. True transport
  unlinkability is the Tor/mix problem; out of scope for now.
- **If a write key is adopted:** delegation-chain format and rotation UX
  (revoke a device → rotate the write key → extend the chain), and
  anti-spam for open mirrors beyond signature-verification (storage
  exhaustion from authenticated-but-garbage heads).
