# Security Model

Zim is designed so storage providers and transport peers can retain encrypted
vault data without receiving authority to read it. This design has not been
independently audited and should not yet be used for sensitive production data.

## Protected Assets

- Vault file and directory contents
- Device private keys
- Vault secrets sealed to shareholders
- Browser keys stored as passphrase-wrapped escrow ciphertext

## Trust Boundaries

### Authorized Devices

Every shareholder can decrypt the vault and sign future versions. Zim does not
currently provide owner, editor, or viewer roles inside a vault. Security
therefore assumes each authorized device is trusted with full vault authority.

### Direct Peers

New inbound vaults are gated through the local contact policy. Downloaded
history is checked through content hashes and manifest signatures. Existing
vault announcements are accepted more broadly, but they do not bypass
cryptographic history verification.

### Hubs

A hub stores signed manifests, encrypted blobs, account metadata, public device
keys, and encrypted browser-key escrow. It cannot passively decrypt a vault
unless one of its own keys becomes a shareholder.

The hub also serves the browser application. A live malicious or compromised
hub can deliver code that captures an unlock passphrase or key, so the
zero-knowledge claim applies to passive storage, not arbitrary malicious code
delivery.

Hub administrators can access service metadata and bypass normal application
ownership checks. Operational control of a hub is a privileged role even when
vault content remains encrypted.

### DID Hosts

A `did:web` account trusts its host to publish the correct current device
roster. Those documents are not independently signed by the account's devices.

## Exposed Metadata

Manifests are plaintext and reveal vault names, authors, shareholder keys,
version relationships, content hashes, and retention information. File names
and directory bodies are encrypted, so the full hierarchy is not directly
visible from the manifest alone.

Hub authorization does not make blob hashes secret. Any authorized hub user who
knows a stored blob hash may request its ciphertext; encryption remains the
confidentiality boundary.

## Revocation Limits

Removing a shareholder prevents receipt of future vault secrets after the next
save. It cannot erase plaintext, historical secrets, or unchanged file keys
already obtained by that device. Removing a device from a hub likewise does not
rewrite existing vault membership.

## Current Limitations

- File bodies use streaming encryption without an authentication tag.
- Native identity keys are not passphrase-encrypted, and secure file
  permissions are not universally enforced automatically.
- Stolen browser escrow enables offline passphrase guessing.
- Browser OAuth lacks some defense-in-depth checks expected in a hardened
  production deployment.
- Public and version-scoped sharing are not shipped.
- Production HTTPS is expected operationally but is not enforced by every
  resolver configuration.

See [Cryptography](cryptography.md), [Identity](identity.md), and
[Vault Architecture](../architecture/vaults/index.md) for the adjacent trust
and implementation boundaries.
