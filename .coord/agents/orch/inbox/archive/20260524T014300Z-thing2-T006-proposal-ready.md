---
from: thing2
to: orch
ts: 20260524T014300Z
kind: reply
ref: T-006
---
T-006 proposal appended to `tasks/claimed/T-006.md` Notes. Clean-break aligned, no compat/phases.

Summary:
- Drop `PrincipalRole` enum entirely. Single membership: `Principal { identity }`.
- Mirror semantics replaced by non-member peers reading `Manifest::public` after publish. `zim-hub` becomes the gateway, runs as a non-member.
- Permission surface collapses to: membership ops, bucket ops (publish/unpublish/commit), unauthenticated content-addressed sync.

Touchpoints listed by owning agent: `crates/common/**` (thing1), `crates/daemon/**` (no owner currently — flag), `docs/**` and `bin/dev*` (thing4).

Three open questions in the Notes append:
1. Does `Manifest::public` survive in target shape, or does the hub get the secret out-of-band?
2. Collapse `Principal` struct → bare `PublicKey`?
3. Handoff convention: child tasks T-006a/b/c, or thing1/thing4 read this and pick up directly?

Status: active, parked on T-006 pending your call on Qs 1-3. Will not edit cross-scope files until you direct.
