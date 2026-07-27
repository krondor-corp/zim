# Trusted-contact auto-share (holder-side minting) — punted

**Stage:** Planned
**Priority:** Medium

## The invariant this must eventually serve

In an E2EE multi-device system, a new key can never retroactively
decrypt: the vault secret reaches a device only as a `SecretShare`
sealed to that device's X25519 key, and envelopes can't be sealed to
keys that don't exist yet. So *some* holder-side minting step —
"a device that has access seals a fresh envelope for the newcomer" —
must exist in every design. The hub can't do it (zero-knowledge); the
DID roster updating doesn't do it (roster ≠ key material).

## What was removed (2026-07)

`reconcile_trusted` — the automated policy "for vaults I own, every
trusted contact's current device set should hold envelopes":

- `crates/zim/src/reconcile.rs` (the sweep)
- `POST /api/v0/peers/reconcile` + `zim peers reconcile`
- auto-triggers after `zim hub peers sync` and on `vaults create`

Removed because the policy ran silently on a fuzzy trigger set and
granted access as a side effect — the same "standing grant" smell as
the scrubbed publication map. View the implementation via git history
(`git log --all -- crates/zim/src/reconcile.rs`).

## Shape to consider when picked up

Make minting explicit and event-shaped rather than sweep-shaped:

- On **enroll** (the one moment a new device appears), prompt/queue:
  "share your N owned vaults to the new device?" — a deliberate act at
  a precise trigger, not a background policy.
- Or a device-initiated **share request** the owner approves.
- Either way the primitive is the same `resolve_reaches` + `add_reach`
  minting used by `vault shares add`; only the trigger and consent
  model change. Relates to
  [versioned-entrypoint share minting](share-minting-versioned-entrypoints.md)
  (minting as explicit act).

Until then: new devices get existing vaults via explicit
`zim vault <v> shares add <did>`; vaults created in the browser still
seal to the whole account roster at creation time.
