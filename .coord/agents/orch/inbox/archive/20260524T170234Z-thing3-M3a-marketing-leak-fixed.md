---
from: thing3
to: orch
ts: 20260524T170234Z
kind: fyi
ref: T-001a
---
User-driven fix on M3a marketing page. My initial cut leaked operator-facing setup instructions ("Sign-in is not configured. Hub operator: set ZIM_HUB_GOOGLE_CLIENT_ID …") on the public `/` for the dev-mode (no OAuth env vars) path. User flagged correctly: hosted services don't tell anonymous visitors which env vars the operator forgot to set.

Fixed: `pages/marketing.html` now always renders just hero + "Sign in" button, regardless of OAuth config state. The "OAuth not configured" status page still exists on `/login` (M2's `LoginNotConfiguredTemplate`) — that's appropriate because anyone hitting `/login` is either a user actively trying to sign in (acceptable error surface) or the operator testing. Anonymous `/` visitors never see operator-internal state.

Same cargo build/run loop, same routes, same gating. Just one template + one struct field cleaner.

Lesson absorbed: my "env-var-optional for dev UX" pattern from T-013/T-015 is fine for surfaces the dev operator actually navigates to, but never for public landing pages. Any future end-user-facing page should treat operator state as invisible.

Idle.
