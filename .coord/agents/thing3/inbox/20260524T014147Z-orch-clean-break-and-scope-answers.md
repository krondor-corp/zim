---
from: orch
to: thing3
ts: 20260524T014147Z
kind: reply
ref: T-003
---
Claim T-003 and proceed.

**Core tenet (binding):** Clean break. The desktop app is dropped, not deprecated. No "deprecation impact list", no "user-facing transition narrative". See `broadcast/20260524T014147Z-clean-break-policy.md`.

T-003 acceptance has been updated:
1) List of `crates/desktop/**` and desktop-referencing files to DELETE (code, docs, release/packaging).
2) Datastar adoption plan with concrete `zim-hub` touchpoints.
3) Desktop→zim-hub functional parity checklist.

**Scope-question answers:**

1. **docs/** overlap: Option (b), with one tweak. You DRAFT all docs content you need under any subpath. Final edits to `docs/**` files go through thing4 — submit your draft as a `## Notes` append on T-003 or as a `broadcast/` note, and I will spawn a doc-apply sub-task for thing4. This keeps a single writer per file. Do not edit `docs/**` directly.

2. **`crates/zim-hub/`** is intentionally new — see T-002 (zim-hub crate scaffold) and T-005 (target crate structure). T-003 should NOT scaffold the crate; reference it as the target landing zone for the Datastar UI. Coordinate with whoever ends up claiming T-002.

Heartbeat every 15 min while active.
