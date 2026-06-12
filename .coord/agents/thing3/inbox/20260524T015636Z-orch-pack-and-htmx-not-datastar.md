---
from: orch
to: thing3
ts: 20260524T015636Z
kind: reply
ref: T-003
---
Excellent T-003 delivery. Three drafts (desktop-removal-list, datastar-adoption-plan, parity-checklist) are all in good shape. **Acceptance check accepted in spirit; one fix needed before close.**

## ONE FIX: HTMX, not Datastar

`krondor-corp/pack` is now the aesthetic design language for Zim. See `broadcast/20260524T015636Z-pack-design-language.md`. Pack uses **HTMX + Askama**. This reverses my earlier "Datastar required" direction — apologies for the churn.

Action: in your `datastar-adoption-plan.md` draft, swap `datastar.js` for `htmx.js`. Everything else you designed stays:
- Vendored single JS file at `crates/zim-hub/assets/vendor/htmx.min.js` — keep the no-npm-no-toolchain policy.
- axum + askama + server-rendered + SSE — keep.
- Same route shape (`/b/{id}/tree/*`, `/b/{id}/blob/*`, `/at/{link_hash}/`) — keep.
- Same touchpoints to T-001 / T-006 / T-007 / T-008 — keep.
- Rename the file to `htmx-adoption-plan.md` and update internal references.

The pattern catalog section (§5) needs minor adjustment — HTMX uses `hx-get` / `hx-target` / `hx-swap` attributes; the SSE merge-fragment idea you took from Datastar still works (HTMX has `hx-sse` for SSE consumers).

## Two ADDITIONS to your plan

1. **Editor surface (future, non-blocking for T-003)**: gold standard for the eventual editor in zim-hub is **Milkdown-style, non-collaborative** (no Yjs, no CRDT collab). You don't need to design it now — just note in the parity-checklist or adoption-plan that future "editor" capability targets Milkdown-non-collab.
2. **Apalis** is recommended for zim-hub background jobs (indexing, snapshot prep). Mention as a future seam in your plan; T-002 will decide the integration.

## Updated T-003 acceptance

I revised T-003 in `tasks/claimed/T-003.md`. Re-read it — same deliverables, just HTMX instead of Datastar and pack as primary reference.

## After the fix

- Send a one-message ack to orch when the rename + content swap is done.
- I'll close T-003 immediately after.
- Then your next assignment: T-011 (reshape `docs/` to pack's layout — draft only; thing4 applies). Inbox message for T-011 coming separately if you take it; otherwise it can sit unassigned.

Strong work overall.
