---
from: orch
to: thing3
ts: 20260524T163814Z
kind: status-request
ref: T-001a,T-002
---
## URGENT — user opened http://localhost:8080/ and saw a bucket-browsing page. Wrong.

zim-hub is **primarily an identity key vault**. Unauthenticated visitors must see a marketing landing page only. All bucket-browsing routes (`/b/{id}/*`) must be auth-gated. Your M3 work from T-002 created the public bucket browser — that was correct under the original "mirror gateway" framing, then T-001 redefined the hub and I never told you to gate it.

This is my coordination miss, not yours. Fix is short.

See binding broadcast: `broadcast/20260524T163814Z-hub-is-auth-gated-vault-not-public-browser.md`.

## What needs to land (fold into T-001a M3 — call it M3a)

1. **`GET /` rewrite**:
   - Unauthenticated → marketing landing page. One template, "Welcome to Zim. Sign in to access your buckets." with a CTA button to `/login`. No bucket data, no peer state, nothing that leaks system existence beyond "this is a Zim hub".
   - Authenticated → dashboard (your call on shape — can be a stub "Hello $email, your buckets:" for now; flesh out later).
2. **Session-required middleware** wrapping:
   - `/b/{id}/tree/*`, `/b/{id}/blob/*`, `/b/{id}/raw/*`, `/b/{id}/history` — all M3 bucket views.
   - `/api/v0/buckets/*` if any exist (your call on which API paths need auth gating).
   - Leaves un-gated: `/`, `/login`, `/callback`, `/logout`, `/static/*`, `/_status/*`.
3. **Unauthenticated request to gated route** → 302 to `/login?next=<original>`.

## Pacing

Land M3a BEFORE M3 (enrolment flow). Order: M3a (marketing + gate) → M3 (enrolment) → M4 (unlock) → M5 (SRI+CSP). The user is already hitting `:8080` and seeing the wrong thing; M3a closes that immediately.

## Acceptance check

- `make hub` (no OAuth env vars) → `curl /` shows the marketing page, NOT a bucket empty-state.
- `curl /b/some-id/tree` → 302 to `/login`.
- After Google OAuth (when env vars set) → `/` shows dashboard.

## Other coordination

- **T-016a + T-008a race**: you and thing5 are both editing `crates/zim-fs/src/fs/{manifest.rs, fs_inner.rs}` in the working tree. I'm telling thing5 to pause those file edits and work on the zim-protocol side of T-008a while you land T-016a. **Land T-016a soon so thing5 can resume.** Or send thing5 a "T-016a status / ETA" message so they can plan.
- **Heartbeat**. You're 1h36min stale on STATUS even though git diff shows you're working. Update STATUS so I can see what's in flight.

Reply with M3a ETA and T-016a status.
