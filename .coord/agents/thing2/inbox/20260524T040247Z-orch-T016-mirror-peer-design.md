---
from: orch
to: thing2
ts: 20260524T040247Z
kind: task-assign
ref: T-008,T-016
---
## T-016 — Mirror as a peer type in the protocol

User correction: "we should not have gotten rid of mirrors because zim-hub needs to act like one."

Decoded: T-006's removal of `PrincipalRole::Mirror` from the share/membership model was right. T-008 handles publish access via per-entry secrets. **But** the protocol still needs a notion of "mirror peer" at the peer-network layer — a non-member peer that holds blobs and serves the published-set without holding the bucket secret. zim-hub IS a Mirror in that sense.

T-016 (`.coord/tasks/open/T-016.md`) is your design task. Same shape as T-008 — design-only, write a proposal in the task's `## Proposal` section, sub-tasks for impl get spawned after.

Key questions in T-016 acceptance:
- Two-layer model: principal = bucket membership; peer-type = wire-level identification.
- Wire verbs available to a mirror peer.
- Owner-side gating logic.
- How does zim-hub announce itself as a mirror? Explicit `mirrors: Vec<PublicKey>` field in manifest, or implicit "anyone with the manifest can fetch published_set blobs"? Pick.

## Sequencing

T-016 is **higher priority than T-001** right now — T-015 (thing3 pivoting zim-hub) and T-007a (thing1 peer library entrypoint) both wait on T-016's design. T-001 (identity model) can come after.

## You've been stale 1h17min

If you're heads-down on T-001 design, that's fine — share what you have so far in a status update so I know not to reassign. If your cron died, the recovery is `/loop /coord tick`.

Heartbeat ASAP.
