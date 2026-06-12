---
from: orch
to: thing6
ts: 20260524T193500Z
kind: fyi
ref: T-004
---
## Welcome. Here's the coordination context.

You're refactoring disparate crypto modules across the workspace into a clean `zim-crypto` core. Per your STATUS + user direction, your current work covers:

- `keys.rs` → module dir split
- `SecretKey` → `PrivateKey` rename (workspace-wide)
- `SharingPublicKey` / `SharingPrivateKey` wrapper types
- Strip `iroh` as inner key type (pure `ed25519-dalek`)
- Remove `iroh-keys` feature gate from zim-crypto; iroh conversions move to call sites in protocol/store

## Coordination context

### Who touches what
- **thing1** owns `zim-peer`, `zim-protocol`, `zim-fs`, `zim-store` — your crypto API changes will break their compile. They're active and responsive; send them a message when you're about to land a breaking public API change so they can fix call sites (or you fix them yourself per your constraints: "fix compile errors from crypto API changes" is in scope).
- **thing5** owns `zim-wasm` — depends on `zim-crypto` with `default-features = false, features = ["wasm"]`. They added `wasm` feature gating around iroh. If you're stripping the iroh-keys feature entirely, zim-wasm's `Cargo.toml` needs the feature reference removed too. Send thing5 a heads-up.
- **thing3** owns `zim-hub` — depends on zim-crypto transitively through the embedded peer. Compile breakage there is thing3's to fix but they need to know it's coming.
- **thing4** owns docs + git — when you're at a green checkpoint, send thing4 a "ready to commit" message.

### Binding policies (read these broadcasts)
- `.coord/broadcast/20260524T014147Z-clean-break-policy.md` — no deprecation, no compat shims. Just make the target shape.
- `.coord/broadcast/20260524T015636Z-pack-design-language.md` + `20260524T015900Z-pack-is-aesthetic-only.md` — pack is aesthetic reference only, not architectural.
- `.coord/broadcast/20260524T040247Z-zim-hub-embeds-peer.md` — hub embeds its own peer; your crypto changes affect that path.

### Recent design decisions affecting your work
- **Single key model confirmed**: Ed25519 for identity + signing; X25519 derived for ECDH/sharing. No separate sharing keypair (user confirmed today — dual-address serialization cost too high).
- **T-017** (device model + JWT): each device has one Ed25519 keypair. `PrivateKey` (your new name) signs JWTs. `SharingPublicKey` (your new type) receives sealed shares.
- **T-008** (per-file/folder publish): `PublicEntry.secret` is a `Secret` (symmetric, ChaCha20). Your rename shouldn't touch `Secret` — that stays as-is (it's the AEAD key, not an asymmetric key).

### Protocol
- Heartbeat STATUS.md on state changes, minimum every 15 min while active.
- Check inbox between work batches.
- One writer per file — if you need to edit outside `crates/zim-crypto/**`, send a message to the owner first or just fix compile errors in-place with a FYI (convention-loosening applies for small cross-scope fixes when the alternative is a broken workspace).

### Your green checkpoint target
When `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --all -- --check` all pass with the new crypto API, that's your checkpoint. Send thing4 to commit.

Go.
