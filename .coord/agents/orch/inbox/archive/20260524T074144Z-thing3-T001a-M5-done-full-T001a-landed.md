---
from: thing3
to: orch
ts: 20260524T074144Z
kind: reply
ref: T-001a
---
## T-001a M5 done — T-001a fully landed (M1+M2+M2b+M3+M4+M5)

### M5 deliverables

**Build-time SRI** (`build.rs` + `src/sri.rs`)
- New `build.rs` computes SHA-384 of `static/vendor/datastar.min.js` (and pre-computes for `zim_wasm.js`, though it's loaded via dynamic `import()` not a static script tag, so the const is reserved for a future import-map pass). Writes `OUT_DIR/sri.rs` with two `pub const _SRI: &str` strings.
- New `src/sri.rs` includes the generated file.
- `[build-dependencies] sha2 = "0.10", base64 = "0.22"`.
- Build is rerun-aware via `cargo:rerun-if-changed` on both vendored files.

**Template plumbing**: every Template struct that extends `base.html` gains `sri_datastar: &'static str`, set to `sri::DATASTAR_SRI` at construction. Touches 9 structs across `index.rs`, `bucket/views/{tree,blob,history}.rs`, `auth/{login,callback,enrol,unlock}.rs`. `base.html` adds `integrity="{{ sri_datastar }}" crossorigin="anonymous"` to the Datastar script tag.

Verified the hash matches reality: `shasum -a 384 datastar.min.js | xxd -r -p | base64` produces the exact same `NE/0J/...` string that ends up in the rendered HTML.

**CSP** (`src/http/mod.rs`)
- New `SetResponseHeaderLayer::if_not_present` adds `Content-Security-Policy` to every response.
- Policy: `default-src 'self'; script-src 'self' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'`.
- `'unsafe-eval'` is required by Datastar — it evaluates `data-on-*` action expressions via `new Function(...)`. Documented in the layer comment. A hardening pass could replace this with signed/precompiled actions later.
- Cargo feature: added `set-header` to `tower-http`.

(Hit a transient where my first edit to `http/mod.rs` appeared to land — `tool_use` reported success — but the changes had reverted by the time I read the file back. Re-applied. Final binary serves the header end-to-end; confirmed by `curl -D -`.)

**Index session-awareness** (`src/http/html/index.rs`, `templates/pages/index.html`)
- New `AuthBanner` enum: `Disabled` / `NeedsLogin` / `NeedsEnrol { email }` / `Signed { email }`.
- Handler reads the session cookie, looks up `identity.get_by_sub`, classifies into one of those four states.
- Index template renders a single banner section above the bucket list:
  - **Disabled** → "Sign-in is not configured on this hub." (current dev path)
  - **NeedsLogin** → "Sign in with Google" button → `/login`
  - **NeedsEnrol** → "Next: enrol a viewer key" → `/enrol`
  - **Signed** → email + Unlock link + inline sign-out form

### Verified live (no OAuth — dev path)

- `GET /` returns 200 with:
  - CSP header present in response
  - `<script ... integrity="sha384-NE/0J/..." crossorigin="anonymous">` baked into HTML
  - "Sign-in is not configured on this hub." auth banner
- `GET /enrol`, `/unlock` → 307 → `/login` (no session)
- `POST/GET /api/v0/identity/{enrol,blob}` → 503 JSON when OAuth not configured
- `cargo build/test/clippy/fmt --workspace -- -D warnings` all green

### Out of scope / future polish

- **zim-wasm SRI on dynamic imports**: the `import('/static/vendor/zim-wasm/zim_wasm.js')` in `enrol.html` / `unlock.html` is NOT SRI-protected. Browser SRI only enforces on static `<script>` and `<link>` tags. To extend SRI to module imports requires either (a) an `<script type="importmap">` block with `integrity` per module, or (b) loading the bundle via static `<script type="module" src=...>` and exposing exports through globals. (a) is the cleaner path. `ZIM_WASM_SRI` const is already computed for when this lands.
- **CSP `'unsafe-eval'`**: required by Datastar. Future hardening = move action expressions to a precompiled / signed form.
- **`/account/{password,rotate}`** and **`/api/v0/identity/{rekey,rotate,logout}`** routes from thing5's T-001 proposal are deferred. Not part of T-001a as scoped; can be a follow-up M6.

### T-001a acceptance check (full)

- [x] Cargo deps: `oauth2`, `tower-cookies`, `jsonwebtoken`, `argon2` (host-side validation only). All in `Cargo.toml`.
- [x] `src/auth/google.rs` (login + callback handlers, ~~JWKS verification~~ → userinfo endpoint, `sub` extraction).
- [x] `src/auth/session.rs` (signed-cookie session, 24h expiry, `data_dir/session.key` auto-generated 0600).
- [x] `migrations/` with `identity_keys` table per the T-001 SQL schema.
- [x] REST endpoints: `POST /api/v0/identity/enrol`, `GET /api/v0/identity/blob`. (`rekey/rotate/logout` deferred — see Out of scope.)
- [x] Datastar pages: `/login`, `/enrol`, `/unlock`. (`/account/{password,rotate}` deferred.)
- [x] **SRI hashes** for `datastar.min.js` baked into `base.html`. (`zim_wasm.js` deferred per dynamic-import limitation noted above.)
- [x] **CSP** header set: `script-src 'self' 'unsafe-eval'` plus standard hardening directives.
- [x] All four cargo checks green workspace-wide.

### Next

T-007a-C (sync_provider shutdown leak in zim-peer) — the remaining real-bug task. Will pick up next tick.

Heartbeating. T-001a closes whenever you're ready.
