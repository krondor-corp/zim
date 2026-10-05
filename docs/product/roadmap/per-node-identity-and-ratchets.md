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

## Read/write share split (optional, unrelated to this)

A read-only vs read-write share distinction (the current model is
all-or-nothing — any shareholder can advance the head) is *possible* but
does **not** belong to this work. Write authorization is already handled
by `author ∈ previous.shares` (see
[metadata-privacy.md](metadata-privacy.md)); a read/write split would be
a per-share capability flag on that check, not a new key. Noted here only
to close the loop — an earlier draft tied it to a "vault write key" idea
that was dropped.

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
- ~~Whether the entry `id` is stored explicitly or the creation-OpId is
  recoverable~~ — resolved in v1: stored explicitly (see below).

## v1 implementation record (2026-10, `alex/ratchet-experiment`)

What actually shipped, and where it deliberately stops short of the
design above:

- **Ratchet library:** `skip_ratchet =0.3.0` (WNFS reference impl),
  wrapped as `zim_crypto::EntryRatchet` — `seed()`, `advanced()`,
  `key()` (blake3 derive-key, domain `zim/entry-content/v1`). Boxed
  internally (~135 bytes of state → 8 bytes wherever it rides). Exact-
  pinned; fork tracked as KRO-224.
- **Identity is intrinsic to the ratchet, not a separate field.** The
  skip ratchet's salt is `H(seed)` and `inc()` never changes it, so
  `EntryRatchet` holds its seed and `id()` *derives* `EntryId` from it
  (blake3 derive-key, domain `zim/entry-id/v1`). Identity and key
  schedule cannot disagree, and nothing extra ships. Upstream keeps the
  salt `pub(crate)`; the fork (KRO-224) can expose it and retire the
  stored seed. *Not* the creation `OpId` — that would need pre-minting
  before the mutation that records the op; provenance stays recoverable
  from the log.
- **`Entry` is `{ link, ratchet, previous, … }` — no stored key, nothing
  optional for identity or keys.** `secret()` *derives* the content key
  from the ratchet by value; `id()` derives identity. `ratchet` is
  required on every entry, the root included (it carries the vault's
  root ratchet — `Entry::root_dir` is gone); `plaintext_hash` is
  required too. `previous` is a plain `Link` whose `Link::default()` means revision 0
  — the same null-link sentinel `Manifest::previous` uses for genesis,
  so the model has one representation of "no predecessor"; the
  `previous()` accessor maps it to `None` so no caller fetches the zero
  hash. No `#[serde(default)]` hedges for data that no longer exists.
- **The writer ships the ratchet — and only the ratchet.** `AddFile`
  carries `ratchet` + `previous`, `Mkdir` carries `ratchet`; the op's
  old `secret` field is gone (derivable) and there is no `id` field
  (derivable). A replaying peer rebuilds
  the writer's *exact* entity and key — peers never advance
  independently (tested: `replaying_ops_rebuilds_the_writers_exact_entities`).
  An `AddFile` without a ratchet (pre-ratchet op) **errors on replay**
  rather than seeding a fresh ratchet, which would derive a key that
  does not decrypt the content.
- **Rewrite = same entity, next revision:** `add` over an existing file
  advances its ratchet (identity unchanged by construction), chains
  `previous = old link`.
  Dir bodies rewritten on the root→leaf path do the same. `mv` moves the
  entry intact (same id/ratchet/link). Synthetic `-p` ancestors seed.
- **A conflict sidecar is a fork of the same entity.** The resolver's
  sidecar must carry the loser's ratchet (the blob is encrypted under
  that key and there is no plaintext to re-key), and identity derives
  from the ratchet — so the sidecar shares the entity id with the file
  it forked from: one lineage, two heads, until a human resolves it.
  Any id-keyed lookup must therefore expect multiple heads after a
  conflict. (Key reuse across the fork is safe: every encryption draws a
  fresh random nonce.)
- **The root is ratcheted too — and that changed the share model.**
  Shares seal the root ratchet **state** (`zim_crypto::RatchetShare`:
  ephemeral X25519 → AES-KW-with-padding over the bincode'd state, plus
  `sealed_at`, the height it derives). A holder brings it forward with
  `inc_by(height − sealed_at)` and derives any later root key itself.
  Consequences, all intentional:
  - `save()` just advances the root ratchet. It **no longer re-mints
    every share** on every save (previously O(shareholders) ECDH per
    save). Shares change only on membership changes.
  - `add_share` seals the **live** root state to the newcomer on the
    spot, stamped with the current height — they read from that version
    onward and nothing before. Grant-from-a-point, by construction. A
    share always carries key material; there is no "granted but not yet
    sealed" state in memory or on disk.
  - `remove_share` / `remove_relay` **re-seed the root lineage** and
    immediately re-seal the fresh state to everyone remaining; the next
    save encrypts under it. Required: a revoked holder could otherwise
    derive every later key. Versions before the revocation stay
    readable through their own manifests' shares.
  - Recovery lives in the shares module: `Share::root_ratchet_at` /
    `Shares::root_ratchet_for` with their own `ShareError` (`NotFound`,
    `SealedAfterManifest`, `Crypto`), surfaced as `VaultError::Share`.
    `Manifest::root_ratchet_for` is a one-line delegate supplying the
    height. The ops-log key is the root key.
  - `SecretShare` (32-byte key sealing) is untouched; the hub and the
    browser envelope still use it for non-root purposes.
- **Rename-aware merge is in.** `Mv` and `Remove` ops carry the moved /
  removed entity's `EntryId` (required — every mv/rm target has one; the
  root refuses both). `Fs::apply_ops` replays the merge window in
  **causal (OpId) order** with an entity → current-path tracker, instead
  of latest-op-per-path iterated in filename sort order. So an `AddFile`
  recorded against a path a concurrent rename vacated is redirected to
  the entity's current path: **an edit follows a rename** rather than
  resurrecting the old name. (Previously the outcome depended on
  whether the new name sorted before or after the old one — silent.)
  Precondition, as `chain::merge` already guarantees: `apply_ops` gets
  the full window, local ops included, since the redirect tracks moves
  across the ops it replays. Tested for both tie-break orderings.
  Known conservative corner: the resolver's conflict check is still
  path-keyed, so two *different* entities colliding on a path after a
  rename can still produce a sidecar — never data loss.
- **Ops carry no `Option`s for identity or key material.** `AddFile` /
  `Mkdir` require the ratchet, `Mv` / `Remove` require the id. "Legacy
  ops" don't exist; the model doesn't pretend they might.
- **Verified:** `make check` green in both CI variants (fuse / no-fuse,
  rustc 1.99); `make e2e` PASS end-to-end — convergence, isolation,
  concurrent forks, FUSE across nodes, restart durability.
