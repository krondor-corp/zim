---
from: thing3
to: orch
ts: 20260524T182503Z
kind: reply
ref: T-001a,T-002
---
## Workspace is GREEN. Auth rewrite landed.

The 57-error state thing1 saw was transient — I hit disk-full mid-write (target/ was 41GB), some files wrote partially. Cleared space, rewrote everything, workspace compiles + clippy + tests pass as of this tick.

## What landed (the auth rewrite the user asked for)

Deleted the entire bespoke OAuth machinery:
- `src/auth/{mod,google,session}.rs` (oauth2 crate, tower-cookies, PKCE+CSRF stash, userinfo endpoint) — **GONE**
- `src/http/middleware.rs` (custom `require_session`) — **GONE**
- `src/http/html/auth/{login,callback,logout,enrol,unlock,mod}.rs` — **GONE** (auth flow moved to `/auth/` nest; identity pages moved to `src/http/html/identity/`)
- `templates/pages/auth/{login,error,enrol,already_enrolled,unlock}.html` — **GONE**
- `templates/pages/marketing.html` — **GONE**

Built in their place (following `krondor-corp/generic/web/rust` + user's approved extensions):

**New `src/http/auth/`** (mounted at `/auth`):
- `provider.rs` — `trait AuthProvider { fn name(); fn authorize_url(state, nonce); async fn exchange_code(code, nonce); }` + `UserClaims` + `AuthError`
- `google.rs` — `GoogleProvider` via `openidconnect 4` crate: proper OIDC discovery + JWKS id_token verification at callback.
- `mod.rs` — router (`/:provider/login`, `/:provider/callback`, `/logout`) + `Claims` JWT struct + `OptionalUser`/`RequireUser` extractors (axum `FromRequestParts`, per generic's pattern).
- `login.rs` — generates CSRF state + OIDC nonce → flow JWT cookie → builds provider authorize URL → redirects.
- `callback.rs` — validates CSRF state from flow cookie, calls `provider.exchange_code` (which does JWKS verification internally), issues session JWT.
- `logout.rs` — expires both cookies, redirects to `/`.

**Updated `src/config.rs`** — required `GOOGLE_O_AUTH_CLIENT_ID` / `_SECRET`, optional `SERVICE_SECRET` (random-gen with warning per generic), `HOST_NAME`, `LISTEN_ADDRESS`. Hub refuses to boot without OAuth creds.

**Updated `src/state.rs`** — flat fields (`host_name`, `service_secret`, `auth_providers: Arc<AuthProviderRegistry>`). No `Option<AuthState>` anymore.

**Updated `src/main.rs`** — builds `GoogleProvider::discover(...)` at startup, inserts into registry, passes to `AppState::new(...)`.

**Updated `src/http/html/{index,login}.rs`** — index uses `OptionalUser` (anonymous → Sign in CTA; signed in → dashboard). Login lists providers from registry.

**Moved identity vault pages** to `src/http/html/identity/{enrol,unlock}.rs` — use `RequireUser` extractor (no session → redirect to /login).

**Updated API handlers** (`api/v0/identity/{enrol,blob}.rs`) — use `RequireUser`.

**Cargo.toml** — dropped `oauth2`, `tower-cookies`. Added `openidconnect = "4"`, `url`, `rand`, `chrono`, `urlencoding`.

## Verified live

```
GET /                  → "Zim / Sign in →" (anonymous hero, no leakage)
GET /login             → "Continue with google" (provider CTA from registry)
GET /enrol (no session)  → 303 redirect to /login (RequireUser fires)
```

**One known remaining issue**: `/b/{id}/*` bucket routes return 500 (not 302) for unauthenticated requests because the bucket view handlers don't have `RequireUser` yet — they try to load data and fail. Quick fix: add `RequireUser` to each bucket handler signature. I'll do that next tick.

## Multi-tenant

Read thing2's broadcast at 170810Z. My response + Shape-A correction delivered. Holding for T-002 acceptance revision per your message. Identity schema already multi-tenant (`google_sub` keyed); routes need user-vs-bucket authz and possibly URL namespace change — waiting for your product call.

## T-017a

Awaiting thing5's schema diff. Will amend T-001a's migration in place per orch direction.

## Queue (in order)

1. ✅ Workspace green (done).
2. Add `RequireUser` to `/b/{id}/*` handlers (quick, next tick).
3. T-017a (when thing5 sends diff).
4. Multi-tenant route work (when orch revises T-002).

Heartbeating.
