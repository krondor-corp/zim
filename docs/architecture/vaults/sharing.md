# Vault Sharing

Sharing combines decryption authority with routing information. The manifest
maps recipient public keys to shares.

## Shares

A share contains:

- the recipient identity,
- the vault secret sealed to that recipient, and
- an optional always-on host through which the recipient is reached.

For a direct share, the recipient is dialed directly. For a hosted share, such
as a browser key, synchronization dials the hub while the secret remains sealed
to the browser. The hub therefore stores ciphertext without becoming a
shareholder.

There is no `dialable` flag and no separate `Relay` manifest object.

## Authority

The manifest has no permanent owner-only ACL. A later manifest is valid when
its signer appeared in the preceding manifest's shares. Every current
shareholder is therefore structurally able to author the next state, including
changing the share set.

Removing a share rotates the vault secret on the next save and excludes that
recipient from future versions. It cannot revoke plaintext or historical
secrets the recipient already obtained.

## DID Expansion

The daemon resolves a supplied DID outside the peer protocol. A `did:web`
identity can expand to several verification methods, producing one concrete
share per recipient key. The peer layer handles only concrete keys, sealed
shares, and reach targets.

## Acceptance Policies

Inbound announcements pass through `AcceptPolicy` before a pull begins:

- A daemon accepts an update for an existing vault, or a new vault from a known
  contact.
- A hub requires both the sender and hosted recipient to be enrolled devices.
- Blob connections are gated separately by the same policy boundary.

HTTP read authorization at the hub checks whether one of the authenticated
user's registered keys is a shareholder in the current manifest.

Trusted-contact records still exist, but automatic trusted-contact sharing is
not wired. Adding a recipient remains an explicit operation; the deferred
consent model is tracked in the
[roadmap](../../product/roadmap/trusted-contact-auto-share.md).
