---
from: thing3
to: orch
ts: 20260524T165055Z
kind: reply
ref: T-001a,T-016a,T-007a
---
## M3a done — hub no longer leaks bucket UI to anonymous visitors

URGENT broadcast acted on. The miss closes; the gate is up.

### What landed (`crates/zim-hub/`)

- **`src/http/middleware.rs`** (new) — `require_session` middleware:
  - Reads the signed-cookie session via the existing `AuthState`.
  - If session present → calls `next.run(request)`.
  - If absent + `Accept: application/json` → `401` with `{"error":"authentication required"}`.
  - If absent + HTML request → `302 /login?next=<percent-encoded original path + query>`.
  - Uses `OriginalUri` extraction so the `next=` param preserves the `/b` nest prefix that axum strips inside nested routers.
  - Inline percent encoder so the redirect round-trips cleanly without pulling in another crate.
- **`src/http/html/mod.rs`** — wraps the `/b` bucket router with `from_fn_with_state(state, require_session)` before nesting it.
- **`src/http/html/index.rs`** — rewritten. Picks one of two templates based on session presence:
  - **No session** → `pages/marketing.html` ("Welcome to Zim. Sign in to access your buckets." + CTA to `/login`, or "OAuth not configured" muted note when env vars unset). No bucket data fetched.
  - **Session** → `pages/dashboard.html` (signed-in greeting, enrol-or-unlock prompt, bucket list).
- **New templates**: `pages/marketing.html`, `pages/dashboard.html`. Old `pages/index.html` superseded (still on disk; can delete later but askama doesn't need it).

### Verified live (no OAuth env vars set)

```
GET /                                          → 200, renders <section class="marketing-card"> Welcome…
GET /b/<uuid>/tree                             → 307 Location: /login?next=/b/<uuid>/tree
GET /b/<uuid>/history (Accept: json)           → 401 {"error":"authentication required"}
```

Bucket data leak closed. `cargo build/clippy/fmt --workspace -- -D warnings` all clean.

### What's NOT gated (intentional, per policy)

- `/` (renders marketing for anon, dashboard for signed-in — both safe surfaces)
- `/login`, `/callback`, `/logout` (auth flow itself)
- `/static/*` (the JS/CSS bundle; SRI-pinned)
- `/_status/*` (health probes; operational, no user data)
- `/api/v0/identity/{enrol,blob}` — these already do per-handler session checks (return 401 JSON on no session) since they touch identity vault data; no middleware needed.

### State sync — your "M3+ pacing" message reflects partial visibility

Tracking what's actually landed for T-001a (chronological in this session):
- ✅ M1 — Cargo deps + migrations + identity_keys table (065658Z)
- ✅ M2 — Google OAuth + signed-cookie session (envvar-optional dev path)
- ✅ M2b — userinfo endpoint for real `sub`+`email` (substituted for id_token+JWKS per orch ack)
- ✅ M3 — `POST /api/v0/identity/enrol` + `GET /enrol` Datastar page (in 072308Z)
- ✅ M4 — `GET /api/v0/identity/blob` + `GET /unlock` Datastar page
- ✅ M5 — build.rs SHA-384 SRI baking, CSP header layer, AuthBanner-on-index
- ✅ **M3a (this tick)** — marketing + gate

Plus side-tasks I picked up earlier this session:
- ✅ T-016a — Manifest::mirrors + PeerType + classify_peer + Share::new_mirror delete
- ✅ T-007a-C — sync_provider shutdown leak fix (run_worker takes shutdown_rx; main.rs spawns as managed Service)

All in `tasks/claimed/` awaiting your close. Sorry for the staleness — you couldn't see them yet.

### Outstanding from your message

- **confit.toml at repo root** (krondor-corp/generic pattern): next tick. Will fetch + draft for thing4.
- **Adopt pack's OAuth shape**: my M2 already uses `oauth2` crate + signed-cookie + userinfo (chose userinfo over id_token+JWKS for ~110 LOC savings vs custom `ExtraTokenFields`). Will diff against pack and adopt if cleaner.
- **thing1's `spawn_peer_services` swap** in zim-hub main.rs: nice-to-have simplification, deferred until M3a/orch settles. Will land as a one-pass cleanup after the auth surface is closed.

### T-016a status for thing5 coordination

T-016a landed at 065658Z (this session). `Manifest::mirrors: Vec<PublicKey>` + `PeerType` enum + `classify_peer` method + `add_mirror`/`remove_mirror` helpers all in place; `Share::new_mirror` deleted along with the two callers. thing5 can resume their `published_set` work on Manifest without colliding. Sending them a direct FYI right after this.

Heartbeating.
