# Per-node identity, versioning, and key ratchets

**Stage:** Design
**Priority:** Medium (high-leverage — unblocks history, rename-merge, and the metadata-privacy work)

## Background: what the model already is

A common misconception — corrected during the design discussion — is that
a vault is encrypted under one "vault secret." It is not. The tree is
already a **Cryptree-style key hierarchy**:

- Each `Entry` (file or dir) carries its **own** `Secret`
  (`Entry::File { link, secret }`, `Entry::Dir { link, secret }`).
- A file's content is encrypted under its per-file key; a dir body is
  encrypted under its per-dir key **and contains its children's keys**.
- Reading descends root → leaf: recover the root secret from your
  `SecretShare`, decrypt the root dir body, which reveals each child's
  key, and so on down.

What we call "the vault secret" is really the **root dir's** secret.
`Vault::save` re-keys only the root with a fresh `Secret::generate()`
(and re-wraps the shares); a child keeps its key unless *it* is
rewritten, in which case the path root → leaf re-keys and untouched
siblings don't. `VaultId` is `blake3(genesis manifest)` — a
self-certifying, derived, permanently stable identity.

So the per-entry key *slot* already exists. Two things are missing, and
they are the whole of this direction:

## The two additions

1. **Stable per-entry identity.** Today a file *is* its path — a rename
   is delete-old + add-new, so identity is lost across a `mv`. Give each
   entry a stable id so it can be followed across renames.
   - The id should not be a bolted-on random UUID: the CRDT already
     mints `OpId = (lamport, peer_id)` for the `AddFile` that created the
     entry. **Reuse the creation OpId as the entity's identity** —
     provenance, not an invented token.
   - You only *don't* need this if you accept rename = new identity
     (losing cross-rename history) — which is the thing we want to gain.

2. **Ratcheted key derivation.** Today each entry's secret is
   `Secret::generate()` (fresh random per rewrite). A **skip ratchet**
   (WNFS/Fission lineage) makes it *derived*: `key_{n+1} = H(key_n)` for
   that entity, with skip levels so a reader can jump N → N+k in
   O(log k) instead of replaying every advance.

## Two chainings — keep them separate

- **Spatial (down the tree)** — the Cryptree wrapping. *Already exists,
  unchanged.* Parent body reveals child keys; access flows root → leaf.
- **Temporal (across revisions)** — the ratchet. *New.* Chains one
  entity's key forward through its own revisions; orthogonal to the tree.
- **Interaction:** on write, a leaf advancing its ratchet must be
  re-wrapped into its parent → parent advances and re-wraps into *its*
  parent → propagates **root-ward**. That is the same root → leaf path
  rewrite that already happens on every save; the only change is each
  node on the path *advances* instead of *randomizing*. Reads still
  descend leaf-ward. Keep stored-spatial + ratcheted-temporal — do **not**
  derive child keys from parent keys (that would force re-keying a whole
  subtree's ancestry on any change; WNFS uses stored child keys for
  exactly this reason).

## Per-file versioning is a separate structure — store `previous`

The ratchet is a **content-independent key clock** (that independence is
what makes it skippable — binding it to blob hashes would kill skip
levels). Versioning is a *parallel* structure indexed by the same
revision counter:

- Add a **`previous` link inside the entry** (prior version of *this*
  entity). Following one file's history is then O(1) — walk its own
  chain — and it survives renames because it's anchored on the entry id.
- The alternative (derive history by scanning the manifest DAG and
  grouping by id) was **rejected**: O(vault-history) per query is too
  expensive for a first-class history UI.

The two compose: to read entity X at revision N, take `key_N` from X's
ratchet and `blob_hash_N` from X's `previous`-chain, then decrypt.

Mental model: **entry id** = the noun · **`previous`-chain** = its
history · **ratchet** = its key over time · the Cryptree tree-wrapping =
the orthogonal access axis.

## Read/write share split (falls out for free)

Introducing a per-vault **write key** (see
[metadata-privacy.md](metadata-privacy.md)) lets a share seal two
capabilities instead of one: the read secret **always**, the write key
**only for writers**. That gives a genuine **read-only vs read-write**
share distinction the current all-or-nothing model can't express.

## What this unblocks

- **History across renames** — supersedes a chunk of the version-history
  UI direction (KRO-209); that UI needs read-at-version + a followable
  per-file chain, which this provides.
- **Rename-aware merge** — concurrent *edit-on-A* + *rename-on-B* can
  finally reconcile as "edit the renamed entity," because the edit and
  the rename now name the same id. (This is the same family as the
  equal-height fork bugs already fixed.)
- **Stable FUSE inodes across sync-driven renames** — a mount can keep an
  entity's inode across a background `mv` that arrives via sync, instead
  of orphaning it. (Local FUSE renames already preserve the inode; the
  gap is sync-driven ones — they don't go through the FUSE `rename`
  handler.) This is a *free side effect*, not a strong enough reason on
  its own.
- The **key ratchet** is the crypto substrate the metadata-privacy work
  builds on.

## Scope / blast radius

Almost entirely **zim-crypto + zim-core**:

- **zim-crypto:** add a skip-ratchet primitive (derive-next + skip
  levels). Self-contained; `Secret` stays.
- **zim-core:** add `id` + `previous` to `Entry`; swap `generate()` for
  ratchet-advance; decide the ops-log key.
- **zim-peer:** one line — `chain.rs` decrypts the ops-log during merge;
  keeps working as long as it gets the right key.
- **zim-api / hub:** zero (ciphertext + opaque shares only).
- **wasm SDK:** zero beyond recompile (pure `Vault::`/`Fs::` consumer).

With no users and throwaway data there is no migration — which is the
"worth doing now" argument. Caveat: it is the crypto core; a skip ratchet
has subtle properties (skip-level structure, the grant-from-a-point
semantics), so treat it as a deliberate design pass, not a quick
refactor.

## Precision notes (corrections from the discussion)

- **Not forward secrecy.** A one-way ratchet gives the *reverse*:
  holding `key_n` derives `key_n, key_{n+1}, …` (forward) but **not**
  `key_{n-1}`. The real property is **grant-from-a-point**: hand someone
  a node's ratchet at revision N and they read N onward, nothing before.
- **Revocation isn't free from the ratchet.** Forward keys are
  derivable, so you revoke by minting a *fresh* ratchet seed +
  re-encrypting (already per-entry-scoped today) — not by advancing.
  The ratchet's win is *efficient catch-up* across missed revisions.

## Open questions

- Ops-log key derivation once entries ratchet (today it rides the root
  secret).
- Skip-level parameters (branching, how far ahead readers commonly jump).
- Whether the entry `id` is stored explicitly or the creation-OpId is
  recoverable without storing it redundantly.
