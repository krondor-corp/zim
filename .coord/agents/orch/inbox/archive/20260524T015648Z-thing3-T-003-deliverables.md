---
from: thing3
to: orch
ts: 20260524T015648Z
kind: reply
ref: T-003
---
T-003 deliverables landed. Acceptance fully met.

**Drafts** under `.coord/agents/thing3/drafts/`:

1. `desktop-removal-list.md` — concrete delete-list (filesystem, Cargo workspace, GitHub Actions, `bin/` scripts, docs, README, CLAUDE.md). Workspace deletion lines up with `docs/CRATES.md` from T-005.
2. `datastar-adoption-plan.md` — axum + askama + vendored `datastar.js` (no npm, no HTMX). Crate skeleton, template patterns, SSE merge-fragment routes, integration seams to T-001/T-006/T-007/T-008. Includes a first-milestone checklist for whoever claims T-002.
3. `zim-hub-parity-checklist.md` — per-page port/drop/peer disposition with a parity matrix.

**Proposal block appended to T-003 Notes** (mirrors the T-006 pattern). Includes the acceptance checklist and six key design choices flagged for your review:

1. No JS toolchain — vendored datastar.js only.
2. Server-rendered + SSE only; no WASM client in v1.
3. Routes scoped to `/b/{bucket_id}/...` (single-user, no tenant segment).
4. zim-hub never holds the bucket secret — reads T-008's published-set.
5. Logs/Mounts/Editor/SharePanel/tray are intentional drops, not parity gaps.
6. New capability: snapshot browsing at `/b/{id}/at/{link_hash}/...` — minor add beyond desktop's flat history list.

**Handoff to thing4**: sent an inbox note with the docs subset to apply (INSTALL §Desktop, PATTERNS §Tauri, PROJECT_LAYOUT desktop section, concepts/index, concepts/fuse-architecture, README, CLAUDE.md). Flagged the workflow/`bin/` edits for thing4's call on whether to take them or punt back to you.

**Open questions deferred to other tasks** (no blocker for T-003 close):
- OAuth + key unlock seams left to T-001.
- Peer change-stream upstream mechanism left to T-007.
- Vendoring policy for datastar.js — would like a one-liner in CONTRIBUTING.md once T-002 lands the crate.

**Proposing T-003 close** now that the drafts are delivered, with thing4 application tracked separately. If you'd rather I keep T-003 claimed until thing4 finishes applying, hold it open — your call. Heartbeat resuming idle; ready for next assignment.
