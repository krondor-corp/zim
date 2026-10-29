# Vault History

Every successful save produces an immutable manifest whose `previous` link
points to its parent and whose height is one greater. Genesis has height zero
and no parent.

## Vault Log

`VaultLog` indexes manifest links by vault and height. Manifest blobs remain the
authoritative history; the log makes heads, probes, and ancestry checks fast.

The log can contain multiple links at one height. Canonical selection chooses
the greatest height, then the lexicographically greatest link at that height.
This is deterministic selection after peers possess the same forks; it is not
itself a merge strategy.

SQLite and in-memory implementations live in `zim-peer`. They preserve forks,
but the concrete append implementations do not currently enforce every
height/parent invariant described by the trait. Chain verification must not
assume an append alone proved continuity.

## Reading Past Versions

Every manifest pins its whole tree, so any historical head fully
reconstructs that version. `Vault::open_at(id, height, ..)` opens the
canonical head at `height` through the same path as `open_with_head`.

Decryption uses the share sealed into *that* manifest. Shares are copied
forward on every save, so a key granted at height `N` can open versions
`>= N` and gets `ShareError::NotFound` for anything earlier — grants are
"from this version onward" by construction. A revocation re-seeds the root
lineage going forward, but the remaining holders' old shares ride along in
the old manifests, so pre-revocation history stays readable to them.

Restoring is never a chain rewrite: read the old content from the
historical handle, write it on the live handle, and save. The restored
content lands at a new height; the overwritten version stays in history.

A file is restorable only if it changed since the version: the old
entry and the live head's entry at that path must not be the same saved
revision (`Entry::is_same_file_revision`). Every saved rewrite advances an
entity's ratchet, and a move reuses the entry, so equal ratchets mean "not
rewritten since". A restore is itself a rewrite, so restored content counts
as changed relative to the version it came from. The browser checks
against the hub's current head when a file is opened and again at restore
time; a fetch failure during the check is an error, never "changed".

The browser exposes this as a read-only `WasmVersion` (no `save`) plus
`WasmVersion::history`, which pages `/api/v0/vaults/{id}/log` and fetches
each manifest's plaintext header for its author and an `Access` of
`Readable`, `Sealed` (no share for this browser — permanent), or
`Unavailable` (the manifest fetch failed — transient, says nothing about
access; the UI keeps it clickable to retry). The web workspace autosaves notes, at most every 30s of editing (plus on file switch, tab hide/close, and Cmd/Ctrl+S), and each write is a version; expect denser history from browser edits than from the daemon. Its History view restores per file —
whole-vault restore would clobber concurrent work in unrelated paths.

Manifests carry height and author but no wall-clock time, so history lists
versions by height. A signed timestamp would need a clock passed in from
the caller (zim-core is wasm-clean and cannot read one itself).

## Operations

Each manifest may point to an encrypted operations log containing only the
filesystem operations accumulated since the previous load or save. Operation
IDs use a Lamport timestamp with the author key as a deterministic tie-breaker.

Current operations add files, create directories, remove paths, and move paths.
Synchronization walks manifests back to an ancestor, decrypts their operation
logs when the local key has access, and merges those operations into a working
tree.

## Conflicts

The current built-in resolver is `ConflictFile`. Two different operations
conflict when they target the same primary path and either operation is
destructive or both add a file.

The greater operation ID wins the original path. When the losing operation is
a file addition, its content can be retained at a deterministic sidecar path
with an eight-character ciphertext-hash suffix. Other losing operation kinds
cannot currently be materialized as sidecars.

There are no current `LastWriteWins`, `BaseWins`, or `ForkOnConflict` built-ins.

## Current Limitations

- `Vault::history` selects one link per numeric height from the log; it does not
  prove those selected links form one ancestry chain.
- Concrete log backends do not validate every sequential-height and parent-link
  rule during append.
- Equal-height divergence is representable in the log but is not discovered by
  the current pull path.
