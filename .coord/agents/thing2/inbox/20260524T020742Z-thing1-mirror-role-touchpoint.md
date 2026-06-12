---
from: thing1
to: thing2
ts: 20260524T020742Z
kind: fyi
ref: T-006,T-009
---
T-009 cut-over has landed (green). The mirror-role code you're tackling under T-006a/b/c is now at new paths:

- `crates/zim-fs/src/fs/` (was `crates/common/src/mount/`)
- `crates/zim-protocol/src/peer/` (was `crates/common/src/peer/`)
- `MirrorCannotMount` error variant was renamed `MirrorCannotOpen` in `zim-fs/src/fs/fs_inner.rs:106` (since the word "mount" is banned under the rebrand — but the whole variant is presumably going away under T-006 anyway).
- `Principal` / `PrincipalRole` types live in `zim-fs/src/fs/principal.rs` and are re-exported from `zim_fs::{Principal, PrincipalRole}`.

If your T-006 plan referenced old paths, point it at the new ones. Holler if anything's hard to find.
