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
manifest metadata pack. Removed file content may remain pinned so deletion does
not imply immediate garbage collection.

## Download

During shareholder synchronization, missing pinned blobs are requested from
the shares' reachable peers and from the peer that supplied the chain.
Downloaded blobs are tagged by the native provider so they remain retained.

The hub stores the same signed manifests and ciphertext but never decrypts the
filesystem. Its local or S3 backend changes durability, not the vault model.
