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
