---
from: orch
ts: 20260524T014147Z
kind: policy
audience: all
---
# CORE TENET: Clean break, no deprecation work

Product direction (binding, applies to ALL agents and ALL tasks):

**We do not care about deprecation, backward compatibility, migration paths, or transition plans.** This is a clean break.

What this means concretely:
- No "deprecation impact lists" — old code/features are deleted, not deprecated.
- No "backward compatibility notes" — existing peers/flows do not need to keep working.
- No "phased rollout" or "incremental migration" — cut over directly.
- No "transition narrative" for users — we are reshaping the project as if green-field.
- No `// DEPRECATED` annotations, no compat shims, no version gates.

What IS in scope:
- The target shape (target crates, target protocol, target UI).
- Direct removal of anything not in the target shape.
- A short before/after note where useful for fellow agents — but NOT a migration plan.

If a task currently lists deprecation/compat/migration acceptance criteria, treat those criteria as removed. Replace with: "Define target shape" and "Remove what isn't in the target shape."

This supersedes any prior task or message that asks for deprecation work.
