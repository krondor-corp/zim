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

## Write authorization — already handled

No new key is needed. `Manifest::verify_author` already gates writes:
the manifest must be **signed by its author** *and* the author must be a
**shareholder on the previous manifest** (`author ∈ previous.shares`).
`write_head` runs this on every advance. Because shares are published,
**any relay reads `previous.shares` and runs the same check** — so
forgery/spam rejection of unauthorized head-advances already exists and
already works for an open, content-blind mirror. A per-vault write key
was considered and **dropped**: since shares are public, it verifies
nothing the existing author-in-shares check can't, so it buys nothing.

(If a read-only vs read-write distinction is ever wanted, it's a
per-share capability flag on the existing model — not a separate key,
and not a privacy feature.)

## Metadata privacy is off the table — by design

This is the blunt consequence of the product decision, stated plainly so
the doc stops implying otherwise. Open, untrusted, high-availability
mirroring requires that any listener see enough to deliver — so:

- **Exposed to any listener (accepted):** vault existence, shareholder
  **membership and routing** (the published shares' `identity`/`via`),
  tree **structure** and blob **count/sizes**, update **cadence**.
- **Protected:** **content only** — Cryptree + per-entry ratchets. That
  is the whole privacy story.

**Cross-vault unlinkability via per-vault derived keys does not survive**
as a real lever, and an earlier draft wrongly listed it as one:

- A daemon's share identity *is* its iroh NodeId (the dial address), one
  per device across all its vaults — already correlated. Deriving
  per-vault identities would mean multi-homing N node identities, and
  they re-correlate at the transport layer (shared relay/IP) that the
  invited untrusted mirrors observe anyway.
- The browser's identity is the account web-key, and a `did:web` account
  is a *stable, advertised* roster by design — per-vault derived keys
  fight the point of it.

So derived keys would only foil an adversary who reads published
manifests but never observes the transport — which is not the adversary
this system has, since it invites untrusted peers onto the transport.
Not worth pursuing for privacy.

## If you want metadata privacy later

It is not free and not compatible with the current product decision. It
would require *either* giving up open untrusted mirroring (mirror needs a
capability → privileged relays), *or* a genuinely different transport
(mix-net-style) so presence doesn't correlate. Both are large and
out of scope; noted only so the boundary is explicit.
