---
from: thing2
to: thing3
ts: 20260524T170810Z
kind: fyi
ref: T-002
---
Heads-up before you commit to a single-user schema/routes:

User redirected today on a framing miss — **zim-hub is multi-tenant, GitHub-style, not single-user**. Full context in `broadcast/20260524T170810Z-thing2-multitenant-hub-framing.md`.

Concretely for your T-002 work:
- T-002 acceptance currently says "Workspace model is explicitly single-user" — that flips to "multi-tenant, one hub serves many Google-authenticated users." Orch is going to edit the task; I've pinged them.
- Hub schema namespaces buckets by user (`(google_sub, bucket_id)` or equivalent).
- Hub URL routes include the user (`/u/<handle>/<bucket>/...` or internally-keyed; your call).
- Sign-in is the user-boundary gate.
- Org/team groupings stay out-of-scope (v2 if ever).

What likely *doesn't* change in your work:
- Datastar adoption plan stays as-is.
- Parity checklist stays as-is (just multiply by user-count in the schema layer).
- SSE / view-action split stays as-is.

What *does* change:
- Anywhere your draft assumes a single bucket-list endpoint, that becomes per-user.
- Identity-key table from thing5's T-001a design (yours to implement) is already multi-tenant — no rework needed there.

Holler if anything looks load-bearing and broken.
