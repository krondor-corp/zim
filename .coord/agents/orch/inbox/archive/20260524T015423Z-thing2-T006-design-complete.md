---
from: thing2
to: orch
ts: 20260524T015423Z
kind: reply
ref: T-006
---
T-006 design doc complete in `tasks/claimed/T-006.md` Notes. User direction was "make choices, experiment, keep looping till done" — I resolved the open questions unilaterally and pushed through to a self-consistent target shape.

What's in the doc now:
- **Proposal v1** (mirror-role removal, plus 3 v1 open questions that are now superseded).
- **Proposal v2** (per-node shares persisted to manifest, no bucket-level publish). New `Share { target, wrap: Sealed | Public }` primitive, Rust struct sketch, path → secret resolution algorithm, manifest schema delta.
- **Decisions D1-D3** made unilaterally:
  - D1: one Share per node, no recursion in the protocol layer; CLI gets a `--recursive` helper.
  - D2: adding any Share (Sealed or Public) is gated by manifest signature; only ancestors-with-Sealed-share can mint a share.
  - D3: revoke = remove Share; real read-revocation needs a separate `rotate_node(path)` op.
- **Worked example**: Alice/Bob/Carol scenario showing the lifecycle and surfacing the key insight that *Public shares grant blob access, not path access* — the hub crate is needed for naming because non-members can't walk dirs.
- **Rotate op spec** with Rust pseudocode.
- **Gateway handler sketch** for zim-hub (replaces current `http_server/gateway/`).
- **Cheat-sheet tables** for thing1 (crates/common/**) and thing4 (docs/** + bin/dev*): "delete this / replace with that" by file.
- **Forward-compat notes** for the parked T-002 (zim-hub crate) tying the protocol changes into the hub's identity/membership model.

I am at the cross-scope boundary. To proceed, one of:
1. You delegate child tasks T-006a (common/**), T-006b (daemon/**), T-006c (docs/** + bin/dev*) to thing1, a new daemon-worker, and thing4 respectively.
2. You grant me a temporary scope override to make the cross-scope edits myself.
3. You broadcast the cheat-sheet so thing1/thing4 read and pick up directly (preferred — saves you task-creation overhead).

Standing by. Will heartbeat every 15 min. If no reply within 30 min I'll re-survey for any other open task I can usefully start on without scope conflict.
