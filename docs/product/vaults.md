# Vaults

A vault is an encrypted, versioned collection of files and directories that can
be synchronized between authorized devices.

## Properties

- **End-to-end encrypted:** Storage providers and hubs retain ciphertext. A
  device needs a valid share to recover vault content.
- **Content-addressed:** Files and versions are identified by cryptographic
  hashes, so downloaded data can be verified independently of its source.
- **Versioned:** Every change produces a signed version linked to its parent.
- **Peer-to-peer:** Authorized native devices synchronize directly when
  reachable. Hosted browser keys synchronize through a hub without giving the
  hub decryption authority.
- **Explicitly shared:** Adding a recipient seals future vault access to that
  recipient. Removing one prevents access to future versions but cannot erase
  data already decrypted or retained.

## Filesystem

Vaults present ordinary file and directory operations through the CLI, HTTP
API, browser, and optional FUSE mount. Those interfaces operate on the same
encrypted filesystem and version history.

Concurrent edits may produce competing histories. Zim retains fork information
and can preserve a losing file addition under a deterministic conflict name,
but equal-height fork synchronization remains incomplete.

## Hubs

A hub improves availability for hosted devices and offline peers by retaining
signed manifests and encrypted blobs. It authenticates users and controls who
may use its storage, but it is not a vault shareholder and cannot decrypt vault
content.

See [Vault Architecture](../architecture/vaults/index.md) for implementation
details and [Security](security.md) for trust boundaries.
