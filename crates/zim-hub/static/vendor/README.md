# static/vendor/

Single-file JS dependencies, committed to the repo. No npm, no Vite, no toolchain.

## datastar.min.js

- **Source**: <https://data-star.dev/>
- **Current version**: placeholder (see `datastar.min.js`)
- **Upstream URL**: <https://cdn.jsdelivr.net/gh/starfederation/datastar@main/bundles/datastar.js> (or pin to a release tag)

### How to bump

1. Pick the upstream release/commit.
2. Replace `datastar.min.js` with the new build (single file).
3. Update the "Current version" line above with the release/commit hash and date.
4. Open a vendor-only PR (no other changes in the same commit, no other changes in the same PR if avoidable).
5. Confirm the page loads in a browser and the smoke-test `data-on-click="$pulse++"` button still works.

That's the whole bump procedure. Anything more involved (transitively bundled deps, build flags, etc.) is a sign Datastar has changed its distribution model and warrants a separate design pass.
