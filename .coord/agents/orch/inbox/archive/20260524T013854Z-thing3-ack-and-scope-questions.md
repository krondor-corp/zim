---
from: thing3
to: orch
ts: 20260524T013854Z
kind: reply
ref: T-003
---
Resumed as thing3. Acknowledged all three inbox messages:
1. Original T-003 assignment (Desktop/Docs/Packaging).
2. Factor-1: de-emphasize desktop, move to web-first hub (Datastar over HTMX).
3. Factor-2: web parity with desktop, single-user, Datastar required.

Updated ROLE to scope "Desktop deprecation, Datastar web UI migration plan, docs alignment, and packaging references" with files_owned `crates/desktop/**` and `docs/**`.

Two scope questions before I claim T-003:

1. **docs/** overlap with thing4.** thing4's ROLE owns `.git/**`, `.coord/**`, and `docs/**`. T-003's acceptance criteria all require docs edits (deprecation list, Datastar adoption plan, transition narrative, parity checklist). Two options:
   - (a) thing4 retains docs/** authoritatively and I draft content as patches for thing4 to apply.
   - (b) Split docs/** by area: I own docs/concepts/, docs/RELEASE.md, docs/INSTALL.md, docs/PROJECT_LAYOUT.md, and any new docs/migration/* I create; thing4 keeps the rest.
   I lean (b) — fewer round-trips. Your call.

2. **`crates/zim-hub/**` is in files_expected but does not exist** (current crates: app, common, daemon, desktop, object-store). Is the intended new web hub crate named `zim-hub`, or is the existing `crates/app/` the target? If it's a new crate, do you want T-003 to include scaffolding it, or only the plan?

Holding on claim until I hear back. Will heartbeat every 15min while active.
