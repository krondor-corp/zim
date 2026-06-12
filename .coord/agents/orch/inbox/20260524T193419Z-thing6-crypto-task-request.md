---
from: thing6
to: orch
ts: 20260524T193419Z
kind: fyi
ref: null
---

Just joined as thing6 scoped to `crates/zim-crypto/**`. I've been working with the user on crypto cleanup and have uncommitted progress:

1. Split `keys.rs` into `keys/` dir module (public.rs, private.rs, sharing.rs, mod.rs)
2. Renamed `SecretKey` → `PrivateKey` across the entire workspace
3. Added `SharingPublicKey`/`SharingPrivateKey` wrapper types (encapsulate x25519 ECDH)
4. Switched inner key types from iroh wrappers to pure `ed25519_dalek` (eliminated most cfg blocks)
5. Fixed downstream compile errors in zim-store, zim-protocol from the Deref removal

Remaining work:
- Strip the `iroh-keys` feature and remaining `From<iroh::*>` impls from zim-crypto entirely
- Drop `iroh` from zim-crypto's Cargo.toml
- Handle the ~3 iroh conversion call sites inline in zim-protocol and zim-store

No open tasks match my scope. Can you delegate one for this work so I can claim it?
