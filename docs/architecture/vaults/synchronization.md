# Vault Synchronization

Vault synchronization is push-notification followed by pull. Peers announce
that a head advanced; the receiver then queries and downloads the authoritative
state itself.

## Transport

The peer protocol runs bincode request/reply messages over iroh QUIC using ALPN
`/zim-peer/0`. Its vault messages cover head lookup, exponential probes,
ancestor lookup, head advancement, and connectivity pings.

`HeadAdvanced` names the vault, announced head, and intended recipient. The
recipient differs from the dial target for a hosted share: the hub receives the
message on behalf of a browser key.

## Announcement And Pull

After a save, the peer announces the new head to each share's reach target.
The receiver:

1. Applies its inbound acceptance policy.
2. Acknowledges the announcement.
3. Enqueues a pull from the sender when accepted.

A pull requests the remote head, compares local height, probes for common
history, downloads and verifies the missing manifest chain, appends links to
the vault log, downloads pinned blobs, merges operations for a shareholder,
and saves a new local manifest. A relay that cannot recover a vault secret uses
a separate log-only path.

Effects run through a bounded in-memory Tokio channel. They are concurrent and
not durably retried.

## Verification

The shareholder path walks actual `previous` links, verifies manifest
signatures, verifies each author against the preceding share set, and checks
the genesis hash against the claimed `VaultId` when it reaches genesis.

The relay-only path verifies the genesis identity when reached but does not
perform the same per-manifest author verification. It must not be treated as
equivalent to opening the vault as a shareholder.

## Reconciliation

Periodic reconciliation retries missed announcements and pulls from distinct
reach targets. The default interval is five minutes, with the first pass after
15 seconds. This is a repair sweep, not the primary write path.

## Current Limitations

- Pull stops when the local height is greater than or equal to the remote
  height, so equal-height forks are not discovered.
- Concrete log appends do not independently validate every height transition.
- The shareholder merge always creates a new local manifest; there is no
  special fast-forward branch.
