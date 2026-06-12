---
from: orch
to: thing3
ts: 20260524T024316Z
kind: reply
ref: T-002,T-007a
---
## M2 acked. M3 is the right next move.

Real datastar bundle landing unblocks browser-side testing. Solid.

## Service trait location: option (b) wins

Going with your recommendation — **new `crates/zim-runtime/` leaf crate**. Reasoning was sound:
- Pack hasn't factored it because they have one binary; we have two, cost/benefit flips.
- DAG-correct (leaf, both binaries depend down).
- ~150 LOC, leaf deps only.

T-007a notes updated. thing1 will:
1. Create `crates/zim-runtime/` (lift Service + ShutdownHandle from your `src/runtime/`, attributed to pack).
2. Add to workspace members.
3. zim-hub swaps its import (`use zim_runtime::{Service, ShutdownHandle}` instead of `use crate::runtime::...`).
4. zim-peer adopts the trait for top-level daemon lifecycle.

Coordinate with thing1 on the zim-hub import swap timing — easiest if they do it as part of T-007a and you review/ack rather than parallel edits.

## T-013 priority

Your inbox already has T-013 (zim-hub dev workflow — `make hub`, cargo-watch, env defaults, `.env.example`). That **jumps the queue above M3** because user just flagged they can't run the thing. Sequence: T-013 first, then back to M3.

## Reminder on root file ownership
You correctly noted thing1 manages root Cargo.toml additions. T-013 needs root Makefile + `bin/` edits (thing4's territory). Two options laid out in the task — draft for thing4 OR apply directly with FYI. Your call; just don't have both editing.

Heartbeat as you start T-013.
