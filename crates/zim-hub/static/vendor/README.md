# static/vendor/

JS / WASM dependencies vendored into the repo. No npm, no Vite, no run-time toolchain. Build-time toolchains (e.g. `wasm-pack` for `zim-wasm`) are acceptable; their outputs land here and get committed.

## Inventory

| Path | What | Per-bundle README |
|---|---|---|
| `datastar.min.js` | Datastar hypermedia client (~30 KB). See below. | (this file) |
| `zim-wasm/` | Browser-side WASM client built from `crates/zim-wasm/`. | `zim-wasm/README.md` |

## datastar.min.js

- **Source**: <https://data-star.dev/>
- **Current version**: `v1.0.0-RC.7` (~30 KB unminified, vendored 2026-05-24)
- **Upstream URL**: <https://cdn.jsdelivr.net/gh/starfederation/datastar@main/bundles/datastar.js> (pin to a release tag once one is published; `@main` is acceptable while Datastar is in RC)
- **Bump command**:
  ```
  curl -fsSL -o crates/zim-hub/static/vendor/datastar.min.js \
      https://cdn.jsdelivr.net/gh/starfederation/datastar@main/bundles/datastar.js
  ```

### How to bump

1. Pick the upstream release/commit.
2. Replace `datastar.min.js` with the new build (single file).
3. Update the "Current version" line above with the release/commit hash and date.
4. Open a vendor-only PR (no other changes in the same commit, no other changes in the same PR if avoidable).
5. Confirm the page loads in a browser and the smoke-test `data-on-click="$pulse++"` button still works.

That's the whole bump procedure. Anything more involved (transitively bundled deps, build flags, etc.) is a sign Datastar has changed its distribution model and warrants a separate design pass.
