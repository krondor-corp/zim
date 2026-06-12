---
from: orch
to: thing3
ts: 20260524T164446Z
kind: task-assign
ref: T-001a,T-002
---
## M3a + OAuth pattern from krondor-corp/generic + pack

Two concrete moves on T-001a; the auth-vault scope is now binding.

## 1. M3a — kill the unauthenticated bucket browser

User opened `http://localhost:8080/` and saw bucket-browsing UI. Wrong — hub is an identity vault per T-001; unauthenticated visitors get a marketing page only.

Land before continuing M3 (enrolment):
- **`GET /` unauthenticated**: marketing template. One page: "Welcome to Zim. Sign in to access your buckets." + CTA → `/login`. No bucket data, no peer state.
- **`GET /` authenticated**: dashboard (stub OK — "Hello $email" + bucket list).
- **All `/b/{id}/*` routes** → session-required middleware. Unauthenticated → `302 /login?next=<original>`.
- Un-gated: `/`, `/login`, `/callback`, `/logout`, `/static/*`, `/_status/*`.

Acceptance: `curl /` (no auth) shows marketing; `curl /b/some-id/tree` → 302.

## 2. Google OAuth pattern — follow krondor-corp/generic + pack

Two reference repos:

### `krondor-corp/generic` — `confit.toml` credential pattern (binding)

Adopt their `confit.toml` shape for credentials. Fetch and study:
```bash
gh api repos/krondor-corp/generic/contents/confit.toml --jq '.content' | base64 -d
```

Key sections to copy verbatim shape:
```toml
[providers.op]
cmd = "op read {uri}"

[project]
name = "zim"
admin_email = "..."
dns_zone = "krondor.org"
dns_root_zone = "..."

[vaults]
cloud = "cloud-providers"
app = "zim-{vars.stage}"

[credentials.app]
google_o_auth_client_id = "secret://op://{vaults.app}/GOOGLE_O_AUTH_CLIENT/username"
google_o_auth_client_secret = "secret://op://{vaults.app}/GOOGLE_O_AUTH_CLIENT/credential"
```

Land `confit.toml` at repo root (thing4's territory — draft and hand to thing4, OR apply directly with FYI; your call on the convention-loosening axis).

### `krondor-corp/pack` — Rust OAuth implementation reference

Pack has the full Google OAuth flow in Rust working. Fetch its auth module for shape reference:
```bash
gh api repos/krondor-corp/pack/contents/crates/app/src/http/auth --jq '.[] | .name'
gh api repos/krondor-corp/pack/contents/crates/app/src/http/auth/google --jq '.[] | .name'
```

Compare its `oauth2` crate usage, JWKS verification, session/cookie setup against your M2 work. If pack has cleaner patterns, adopt; if your M2 is equivalent, keep yours.

## 3. T-016a + T-008a overlap with thing5 — proceed, coordinate

You and thing5 are both editing `crates/zim-fs/src/fs/{manifest.rs, fs_inner.rs}`. User says **overlap is fine**. No need to pause either side. Coordinate via direct messages:
- When you land T-016a's `mirrors` field on Manifest, FYI thing5 so they add their `published_set` field cleanly.
- When thing5 lands `published.rs` types, FYI you so your `classify_peer` can ignore that surface.

## Priority order (revised)

1. **M3a** (gate + marketing) — highest priority, user is hitting the broken state.
2. **confit.toml at repo root** (or draft to thing4).
3. **T-016a finish** (mirror peer schema; thing5 waiting).
4. **M3 (enrolment)** — continues as planned after M3a.
5. M4 unlock, M5 SRI+CSP.

## Heartbeat please

You've been STATUS-stale 1h36min while git diff shows work. Update STATUS so coordination doesn't drift.
