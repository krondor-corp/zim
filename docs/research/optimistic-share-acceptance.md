# Optimistic share acceptance — tentative log entries

**Status:** design note. Not implemented. The current `ShareOffered` flow
synchronously walks the chain back to genesis before the receiver's
`vaults list` shows anything, which means a slow or offline announcer
blocks the recipient's read path.

## Problem

When alice's daemon pushes `ShareOffered { vault_id, head, height }` to
bob, bob today bootstraps the vault via:

1. fetch head manifest from alice,
2. walk `previous` links all the way to genesis, appending each parent,
3. only then is the vault openable / readable.

If alice goes offline mid-walk, bob is stuck — the vault is either
invisible (`list_vaults` returns nothing because the log isn't
populated) or in an inconsistent half-bootstrapped state.

The asymmetry to fix: **reading the vault as it is right now** only
needs the head manifest. Chain history is a sync-time concern. Read
latency shouldn't wait for verification.

## Proposal

Introduce a `verified: bool` (or equivalent two-state) marker on log
entries. Bob's flow becomes:

1. Receive `ShareOffered`.
2. Download head manifest. Verify alice's signature on it.
3. `log.append_tentative(vault_id, name, head_link, height)` — no
   `previous` constraint, no chain check. Returns immediately.
4. Vault registers in the coordinator and becomes openable / readable
   right now.
5. Background `Effect::VerifyChain` walks `previous` links, appending
   each parent and promoting them to verified as it goes.
6. If verification stalls past a TTL (parent blob unfetchable,
   announcer unreachable), the tentative entry is evicted. The vault
   disappears from `vaults list` until alice re-offers. In-flight
   readers holding a cached working copy stay alive.

## Surface changes

- `VaultLog::append_tentative(id, name, link, height)` — no parent
  arg, no chain enforcement.
- `VaultLog::promote_to_verified(id, link)` — flips a single entry.
- `VaultLog::evict(id)` — removes all entries for a vault. Used by the
  TTL watchdog and any future "rejected share" path.
- `VaultLog::list_vaults` should probably surface verification state
  per vault so the CLI can render `demo (tentative)`.
- Bob's `handle_share_offered` skips the synchronous chain walk in
  favour of `append_tentative` + a background `Effect::VerifyChain`.

The wire message (`ShareOffered`) does **not** change.

## Trade-off the reader should understand

Reading a tentative head trusts only alice's signature on that one
manifest. If the chain history would have revealed a key rotation
event (a prior owner handed authority to alice), bob is reading a
manifest signed by a key whose authorisation he hasn't traced. For
the "alice is a trusted co-owner the moment she shares" threat model
this is fine — she signed with the key she has now. If you want
stronger guarantees ("the chain proves alice was authorised to rotate
at some point in history"), reads must stay blocked until
verification completes; the optimistic path becomes opt-in instead of
default.

## When to revisit

After the "spam + synchronous bootstrap on receive" v1 lands and we
see real two-peer flows. The optimistic path is the right answer
for production but the synchronous bootstrap is the right
short-circuit for getting peer-to-peer working at all.

Original context for this note: came up while extending the sync
protocol with `WireRequest::ShareOffered`. The synchronous bootstrap
is the v1; this document captures the v2 design so we don't lose it.
