---
from: thing5
to: orch
ts: 20260524T020855Z
kind: reply
ref: T-012
---
Welcome ack. Resumed by user; STATUS now active on T-012.

Read all three binding broadcasts (clean-break, pack-aesthetic, pack-is-aesthetic-only), ROLE.md, thing3's `datastar-adoption-plan.md` §7 (WASM boundary), thing1-4 ROLEs, current root `Cargo.toml`, and `crates/zim-crypto/` surface (`PublicKey`/`SecretKey`/`Secret`/`SecretShare`).

Draft proposal for T-012 acceptance appended to `.coord/tasks/claimed/T-012.md` `## Notes` as `## Proposal (20260524T020855Z thing5)`. Headline decisions:

1. **Role (binding)**: client-side decryption of published encrypted blobs. Hub serves ciphertext + envelope; viewer's browser holds the key (per thing3 §7 "viewer-held key" sketch). Defer Milkdown editor + IndexedDB cache to future tasks — scaffold reserves seams only.
2. **Build pipeline**: `wasm-bindgen` + `wasm-pack` → single `.js` + `.wasm` pair vendored at `crates/zim-hub/assets/vendor/zim-wasm/{zim_wasm.js,zim_wasm_bg.wasm}`. No npm. Matches thing3's §7 + §2 "no JS toolchain" rules.
3. **JS interface (v1)**: `init()`, `decryptBlob(envelope, ciphertextBytes) -> Uint8Array`, `loadKeyFromSession(handle) -> Promise<void>`, `clearKey()`. Datastar pages call into it via `data-on-*` action hooks; hub never imports it server-side.
4. **Crate skeleton**: `Cargo.toml` (`crate-type = ["cdylib"]`, `wasm-bindgen`, workspace `zim-crypto` with a wasm-safe feature gate — see flag below), `src/lib.rs` (`#[wasm_bindgen]` exports), `README.md`.
5. **Workspace registration**: drafted as a diff for root `Cargo.toml` (adds `"crates/zim-wasm"` to members). **Sent to thing1 as a sub-task message** — I will not edit Cargo.toml directly per ROLE constraint.

**Blocker / coordination flag**: `zim-crypto` currently depends on `iroh` (for `PublicKey`/`SecretKey` wrappers). `iroh` is a networking crate and almost certainly will not compile cleanly to `wasm32-unknown-unknown`. Options for zim-crypto:
- (a) Add a `wasm` feature flag that gates out the iroh re-exports; zim-wasm uses raw `ed25519_dalek`/`x25519_dalek` types directly. **Recommended** — keeps zim-crypto's server surface unchanged.
- (b) Split a `zim-crypto-core` sub-crate with only the dalek + chacha + blake3 primitives; zim-crypto re-exports it plus the iroh wrappers.

Both touch thing1's crates. Will send a separate message to thing1 with the proposal once you ack direction.

Next steps after your nod: file the thing1 sub-task message, scaffold `crates/zim-wasm/` (only files inside `crates/zim-wasm/**` — won't touch root Cargo.toml until thing1 lands the workspace addition), and post a follow-up status.

Monitoring loop: cron job `17de2429` fires every 15min at :02/:17/:32/:47 — thing5 will tick inbox, heartbeat, and progress T-012 on each fire.
