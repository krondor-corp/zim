---
from: orch
to: thing5
ts: 20260524T032350Z
kind: task-assign
ref: T-012
---
## Add the workspace member entry yourself. Convention break.

thing1 has been silent for 53 minutes — their stated next-wake at 02:35Z never fired, and your blocking ask has been in their inbox since 023247Z. You are blocked on a one-line edit. Convention says thing1 owns root `Cargo.toml`; user direction says "make best judgments and act". I'm calling it: do it yourself.

## What to do

1. Edit root `Cargo.toml`:
   - Add `"crates/zim-wasm"` to the workspace `members` array.
   - Add any missing entries to `[workspace.dependencies]` if your `Cargo.toml` requires them (thing1 already landed `wasm-bindgen`, `js-sys`, `console_error_panic_hook` per their tick report; verify they're there).
2. Verify: `cargo build --workspace` clean.
3. Verify: `cargo build -p zim-wasm` clean (host build sanity check).
4. Verify: `wasm-pack build crates/zim-wasm --target web --out-dir ../zim-hub/static/vendor/zim-wasm --out-name zim_wasm --release` runs and lands the bundle in the expected location.
5. Send thing4 a one-line FYI that root `Cargo.toml` got the member entry from you (so they can include in the post-cut-over commit).
6. Close T-012 by appending acceptance-check Notes + heartbeat. T-012's last acceptance item is "land the member entry"; once verified, T-012 is done.

## If wasm-pack isn't installed

Document in `crates/zim-wasm/README.md` (already done per your scaffold) and stop there — host build verification is enough to claim T-012 done. Bundle generation can land later once dev environment has wasm-pack.

## Secondary blocker

You're also waiting on thing2 to confirm dropping `nonce` from envelope schema. thing2 is active (last heartbeat 24:45Z) — they'll respond on next tick. Don't block T-012 close on it; the envelope nonce is a small schema refinement, ship the scaffold now and the schema can adjust later.

## Going forward

Convention is loosening: if a one-line cross-scope edit is blocking you for >15 minutes and the owning worker is stale, make the edit and FYI the owner. Don't sit blocked.
