# Vault Storage

Vault storage separates transport-independent content operations from native
blob providers.

## Blob Boundary

`zim-core::BlobStore` is the storage boundary used by vault and filesystem
code. It supports fetching, inserting, streaming inserts, and stat checks, with
helpers for raw bytes and DAG-CBOR values. `zim-core` does not depend on iroh.

`zim-peer::BlobsProvider` implements that boundary and exposes iroh's blob
protocol. Native providers support memory, local object storage, and
S3-compatible object storage; local and S3 providers use a SQLite object index.

## Filesystem Content

An opened filesystem uses two storage tiers:

1. A metadata pack containing encrypted directory bodies. The current pack is
   held in memory and embedded into the next manifest.
2. The inner blob store containing encrypted files, operations logs, signed
   manifests, and directory bodies not available in the current metadata pack.

Keeping current directory bodies in the manifest lets a reader traverse the
tree after fetching the manifest while file bodies remain independently
content-addressed.

## Pins

Pins are an inline ordered set of blob hashes in the manifest, not a linked
HashSeq. They identify external content that should remain available.

A save pins referenced file ciphertext, a newly written operations log, and
the immediately previous manifest. Directory bodies are carried in the
manifest metadata pack. A version's pins describe exactly what that version
needs: overwriting or removing a file (or a whole subtree) drops its pins, and
earlier versions' operations logs are not carried forward. Content an older
version still references remains pinned by that version's own manifest, so
dropping a pin here does not imply garbage collection.

## Download

During shareholder synchronization, missing pinned blobs are requested from
the shares' reachable peers and from the peer that supplied the chain.
Downloaded blobs are tagged by the native provider so they remain retained.

Which pins a pull fetches is the peer's pin policy (`PinPolicy` in
`zim-peer`'s `content_cache` module):

- **Full** (default; every `zim` daemon and the hub) — every pin is
  downloaded on pull, and the reconcile sweep re-downloads any that are
  missing.
- **OnDemand { budget_bytes }** — for space-constrained peers (the native
  iOS peer, laptops with large vaults). A pull downloads only structural
  pins: the operations log and the previous manifest. Directory bodies
  already ride in the manifest, so the tree can be listed with no content
  local. The sweep heals the same structural set and never content.

Reads go through `SyncCoordinator::cat`. On a local miss it fetches the
file's ciphertext from the shares' reach targets plus the peer the vault was
last pulled from. If none of them can serve it, the read fails with
`ReadError::ContentUnavailable`, which the daemon's cat endpoint returns as
503. Under `Full`, this same path heals a pin the sweep hasn't reached yet.

Under `OnDemand`, blobs fetched on read go into an in-memory LRU. When fetched
content exceeds the budget, the least recently read blobs are deleted from
the local store; the blob just fetched is never deleted. The LRU tracks only
blobs it fetched from a peer. Locally authored content is never a candidate,
so a write nobody has acknowledged can't be evicted (the cost: authored
content never counts toward the budget). The LRU isn't persisted, so content
fetched before a restart stays on disk and is never evicted. Persisting the
LRU is follow-up work.

The hub stores the same signed manifests and ciphertext but never decrypts the
filesystem. Its local or S3 backend changes durability, not the vault model.
