# Vault Architecture

A vault is an immutable manifest history plus encrypted filesystem content.
Opening one combines the current manifest, the local shareholder key, a
decrypted working tree, blob storage, and a history index.

## Boundaries

- `zim-core` defines `Vault<B, L>` over the `BlobStore` and `VaultLog` traits,
  along with manifests, filesystem entries, shares, pins, and operations.
- `zim-peer` supplies native blob and log implementations, iroh transport, and
  synchronization coordination.
- `zim` supplies daemon orchestration, DID resolution, HTTP operations, and
  optional long-lived FUSE writers.
- `zim-hub` mirrors signed manifests and ciphertext without opening vaults or
  recovering their secrets.

`zim-peer::Vault<L>` is only an alias for
`zim_core::vault::Vault<BlobsProvider, L>`; there is one vault abstraction.

## Lifecycle

1. Load the canonical manifest link from `VaultLog`.
2. Verify the signed manifest and recover the local recipient's vault secret.
3. Decrypt the filesystem into a mutable working tree.
4. Apply filesystem operations in memory.
5. Save encrypted content and a newly signed manifest.
6. Append its link to `VaultLog` and announce the new head to shareholders.

Ordinary HTTP operations reopen the canonical head per request. A FUSE mount
retains an `Arc<RwLock<Vault<...>>>` so mounted writes share one writer.

## Reading Order

| Document | Purpose |
|---|---|
| [Data Model](data-model.md) | Vault identity, manifests, working trees, and save lifecycle |
| [Storage](storage.md) | Blob abstraction, encrypted content, metadata packs, and pins |
| [History](history.md) | Manifest chains, heads, forks, operations, and conflict handling |
| [Sharing](sharing.md) | Shareholders, routing, revocation, and acceptance policies |
| [Synchronization](synchronization.md) | Announcements, pulls, chain verification, and reconciliation |
