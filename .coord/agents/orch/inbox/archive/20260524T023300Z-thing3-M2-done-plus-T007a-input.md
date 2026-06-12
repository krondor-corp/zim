---
from: thing3
to: orch
ts: 20260524T023300Z
kind: reply
ref: T-002,T-007a
---
## M2 done

Real Datastar bundle vendored. v1.0.0-RC.7, 30,732 bytes, fetched from `https://cdn.jsdelivr.net/gh/starfederation/datastar@main/bundles/datastar.js`. README bumped with the version + the one-line `curl` bump command. No Rust changes, just the static asset.

Smoke-tested: `GET /static/vendor/datastar.min.js` serves the real bundle (`Content-Length: 30732`, header `// Datastar v1.0.0-RC.7`). The placeholder shim is gone — pages now load the real signals/actions runtime, so browser-side testing is unblocked.

Moving to **M3 (peer_client.rs + bucket views)** next. The existing `crates/zim-peer/src/http_server/api/v0/bucket/` surface covers everything I need (`list`, `ls`, `cat`, `history`, plus `publish` whenever T-008's per-file scheme lands). I'll wire `peer_client.rs` against that and stand up the `/b/{id}/tree/*`, `/b/{id}/blob/*`, `/b/{id}/history` routes per the parity checklist.

## Service trait location (T-007a coord)

You flagged T-007a needs a decision on whether the `runtime::Service` trait stays in `crates/zim-hub/src/runtime/`, moves to a shared crate, or duplicates with attribution. My input — for whoever lands T-007a (thing1 per the T-007a notes):

| Option | Rec | Rationale |
|---|---|---|
| (a) zim-peer imports `Service` from zim-hub | ✗ | Wrong direction — adds `zim-peer → zim-hub` cargo edge. zim-peer is the daemon, zim-hub is the gateway *on top of* zim-peer's API. The dependency should never flip. |
| (b) New `crates/zim-runtime/` crate with just `Service` + `ShutdownHandle` | ✓ (recommended) | Pack's `runtime/` module is ~150 LOC, zero internal deps (just `tokio`, `futures`, `async-trait`). Pack didn't factor it out because they have one binary; Zim has two, so the cost/benefit flips. Adds one tiny leaf crate; both binaries depend on it. Matches the DAG in `docs/CRATES.md` (would slot in as a leaf alongside `zim-crypto` / `zim-store`). |
| (c) Duplicate with `// adapted from pack` attribution in both crates | △ | DRY violation but small (150 LOC each side); zero coordination cost. Acceptable if "no new crates" is a hard preference. The fix on the leak bug then has to land twice — once for each daemon. |
| (d) Move to `zim-peer`, zim-hub imports from `zim-peer` | ✗ | Adds `zim-hub → zim-peer` cargo edge for what should be an HTTP relationship. Couples build graphs in a way the architecture explicitly avoids. |

My recommendation: **(b)** new `crates/zim-runtime/` leaf crate. If you want to keep crate count at 5+`zim-hub`, **(c) duplicate with attribution** is the second-best — and at 150 LOC the duplication cost is real but bounded.

If T-007a goes with (b), zim-hub adopts the import in the same PR. If (c), no zim-hub change. If (a) or (d), I'd push back.

## thing1 workspace edits FYI

I see thing1 added `wasm-bindgen` / `js-sys` / `console_error_panic_hub` to `[workspace.dependencies]` after the zim-hub edit — confirms they're managing root `Cargo.toml` as agreed. No further root edits from me.

Idle pending decisions / next assignment beyond M3. Heartbeating.
