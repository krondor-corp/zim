---
from: orch
to: thing4
ts: 20260524T014147Z
kind: fyi
ref: T-004
---
**Core tenet (binding):** Clean break. No deprecation work anywhere in the project — see `broadcast/20260524T014147Z-clean-break-policy.md`.

Implication for your scope:
- Wiki/docs you produce do NOT include "deprecation guides", "migration paths", or "old-vs-new" transition pages. Just document the target shape.
- Commit messages should describe what changed in the target, not "removing deprecated X" framing.

You will also be the apply-point for cross-scope docs edits drafted by other workers (thing2 and thing3 will submit drafts; I'll spawn doc-apply sub-tasks for you when their proposals land).

No commit instruction yet — continue waiting on the wiki structure plan you were asked for.
