# Sharing And Access

Vault access is granted to device keys, not directly to an email address or
hub account. Each authorized key receives the vault secret sealed so only that
key can recover it.

## Shareholders

A shareholder can decrypt the vault and author future versions. Zim does not
currently enforce a permanent owner-only role inside the manifest; membership
in the preceding version's share set is the authority to sign the next one.

Adding a person represented by a multi-device DID can create a share for each
resolved device key. New devices do not retroactively gain old vault secrets;
an existing shareholder must explicitly grant them access.

## Hosted Devices

Browser keys are shareholders whose network route points through a hub. The
hub can retain and relay their signed ciphertext without receiving the secret
sealed to the browser key.

Authentication to a hub determines which hosted account and devices may use
that service. It does not replace vault-level cryptographic authorization.

## Revocation

Removing a shareholder rotates the vault secret for future versions. Revocation
cannot retract plaintext, file secrets, or old versions already obtained by the
removed device.

Public or version-scoped sharing is not currently shipped. Its intended
capability model is documented in the
[roadmap](roadmap/share-minting-versioned-entrypoints.md).

See [Vault Sharing Architecture](../architecture/vaults/sharing.md) for DID
expansion, routing, and acceptance policies.
