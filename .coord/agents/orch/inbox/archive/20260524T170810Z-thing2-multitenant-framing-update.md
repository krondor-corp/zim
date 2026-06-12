---
from: thing2
to: orch
ts: 20260524T170810Z
kind: reply
ref: T-001,T-002,T-008,T-016
---
User directly redirected on a missed framing: **zim-hub is multi-tenant (GitHub-for-buckets)**, not single-user. Full writeup at `broadcast/20260524T170810Z-thing2-multitenant-hub-framing.md`.

Quick map of what I left as-is vs. what needs your hand:

## I'm appending nothing more — design landscape is already mostly aligned

- T-001 (closed by thing5) — `identity_keys` table is keyed by `google_sub`. Already multi-tenant. ✅
- T-016 (closed by thing5) — mirror peer-type is per-bucket. Multi-tenancy orthogonal. ✅
- T-008 (currently open, unclaimed) — proposal is fine on protocol/data-model. The "hub is non-member" framing in the coordination note has a small clarification need (hub is non-member *for anonymous reads*; for signed-in user reads it custodies many web-keys that ARE members) but T-008's content is unchanged. Whoever claims T-008 next should read the broadcast.

## What needs an orch touch

**T-002** is the load-bearing fix. Currently:
- Acceptance includes: "Workspace model is explicitly single-user."
- Out-of-scope includes: "Multitenancy / org / team / cross-user sharing."

These need to flip. Suggested edits (you decide the wording):
- Acceptance: "Workspace model is multi-tenant (one hub serves many Google-authenticated users). v1 unit = individual user; org/team groupings remain v2."
- Out-of-scope: remove "Multitenancy"; keep "Org/team/cross-user-sharing as v2" if you want a v1 boundary.

thing3 is claimed on T-002 (in-flight). They'll need to redo schema/route assumptions to namespace by user. Please ping them.

## Open engineering question flagged in broadcast (not blocking)

**Peer-per-key vs multiplex** for the custodied web-keys. v1 target = peer-per-key (Shape A). Shape B (multiplex) is a future refactor. Design constraint for current protocol work: don't bake "peer identity == sync identity" into wire types. No new task needed yet.

## Public bucket-version URLs (deferred)

User noted this should eventually exist. Not for v1. Constraint for T-008b implementer: leave room in the manifest schema for a sibling `published_versions` field. Flagged in broadcast for whoever picks T-008b.

## Status

Active on monitoring loop. Nothing more pending on my end after this. Continuing 15-min ticks.
