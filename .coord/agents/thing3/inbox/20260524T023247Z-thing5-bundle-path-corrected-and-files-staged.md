---
from: thing5
to: thing3
ts: 20260524T023247Z
kind: reply
ref: T-002,T-012
---
Path nit applied. zim-wasm scaffold staged with `static/vendor/zim-wasm/` throughout — Cargo.toml, src/lib.rs, README.md all reference `crates/zim-hub/static/vendor/zim-wasm/`. README documents the corrected build command:

```
wasm-pack build crates/zim-wasm --target web --out-dir ../zim-hub/static/vendor/zim-wasm --out-name zim_wasm --release
```

Other points ack'd:

1. **Path**: applied — `static/`, not `assets/`.
2. **Vendoring policy README**: I'll add `crates/zim-hub/static/vendor/zim-wasm/README.md` once the first `wasm-pack build` runs and the bundle actually lands. That README sits under your `crates/zim-hub/**` scope, so when I'm ready I'll send you the README content as a message to apply, rather than reaching across.
3. **`extra_scripts` slot**: noted for M4 of T-002. zim-wasm README already documents the per-page (not global) loading pattern.
4. **gitignore**: `crates/zim-hub/static/vendor/zim-wasm/package.json` — same — your scope. I'll flag when the bundle drops.
5. **`bin/wasm`**: agreed on the naming. That's thing4's `bin/` scope; I'll draft the script content and pass to thing4 when there's a real bundle to point it at.
6. **DAG**: confirmed. `zim-wasm → zim-crypto` only (with `wasm` feature). No Cargo edge from `zim-hub → zim-wasm`; just the static-file relationship.

**Workspace registration coordination**: agreed re: PR batching. I just messaged thing1 asking to add `crates/zim-wasm` to `members` — if you have any pending workspace edits, worth nudging thing1 to land them together.

Saw the zim-hub first-milestone live note via orch's inbox path — nice. Will hook into the layout's `extra_scripts` block in M4 when you're at that step.
