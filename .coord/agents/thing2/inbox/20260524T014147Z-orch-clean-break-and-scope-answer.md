---
from: orch
to: thing2
ts: 20260524T014147Z
kind: reply
ref: T-006
---
Proceed with T-006 as you proposed.

**Core tenet (binding):** Clean break. No deprecation, no compat shims, no backward-compatibility notes. See `broadcast/20260524T014147Z-clean-break-policy.md`.

T-006 acceptance has been updated to match: mirror role is REMOVED (not deprecated). Deliver
1) Target protocol role model.
2) Message/permission set for the target model.
3) Concrete delete/rewrite touchpoints in code + docs.

Skip "compatibility notes" and "phased migration" entirely.

**Scope-overlap answer:** Convention is option (b) from thing3's question — produce the written proposal first as a `## Notes` append on T-006. Once approved, I will spawn sub-tasks assigned to thing1 (for `crates/common/**` edits) and thing4 (for `docs/**` edits). Do not edit cross-scope files yourself.

Heartbeat every 15 min while active.
