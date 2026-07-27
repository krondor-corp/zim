# `zim vault export` — one-shot decrypt-to-disk dump

**Stage:** Planned
**Priority:** Medium

## Objective

`zim vault <target> export <dir>`: materialize a vault's entire tree as
plain decrypted files under `<dir>`.

## Background / prior art

The archived `_zim-peer` had this working (`crates/_zim-peer/src/http_server/api/v0/bucket/export.rs`,
esp. `export_mount_to_filesystem`, lines 98-159): `ls_deep("/")`, then per
`Entry::File` fetch ciphertext blob → `secret.decrypt` → write plaintext,
recreating the dir tree; returned `{name, link, height, files_exported}`
plus a path→(blob_hash, plaintext_hash) map. Use it as the spec; the
code is bucket-era (UUID ids, old Fs API) — reimplement against the live
`Vault`/`BlobStore`.

## Implementation sketch

1. Daemon HTTP op `POST /api/v0/vault/:id/export {target_dir}` following
   the existing vault-op pattern (typed request in the mounts/vault
   style; Op + CLI verb).
2. Walk `fs().ls_deep`, decrypt each file entry via the vault secret,
   write to disk. Dirs from the tree shape.
3. POSIX-style CLI output (silent on success — see
   [Vault POSIX command UX](vault-posix-command-ux.md)).

## Intended outcomes

- `zim vault demo export /tmp/demo-dump` reproduces the tree,
      byte-identical plaintext.
- Errors (missing blobs, unwritable target) fail the whole op with a
      clear message; no partial-silent success.
- Works on a relay-less daemon (no hub required).

> The archived crate was deleted after the 2026-07 salvage audit — view the cited files via git history (`git log --all -- 'crates/_zim-peer'`).
