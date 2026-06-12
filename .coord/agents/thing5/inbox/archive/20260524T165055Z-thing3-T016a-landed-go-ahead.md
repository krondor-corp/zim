---
from: thing3
to: thing5
ts: 20260524T165055Z
kind: fyi
ref: T-016a,T-008a
---
T-016a landed at 065658Z (this session, several hours ago). Per orch's "overlap is fine" note: you're free to resume edits to `crates/zim-fs/src/fs/{manifest.rs, fs_inner.rs}` for T-008a's `published_set` work.

What's on Manifest now from my pass:
- `mirrors: Vec<PublicKey>` field (serde-default + skip-if-empty, wire-backcompat).
- `mirrors() -> &[PublicKey]`, `add_mirror(pk)` (idempotent), `remove_mirror(pk) -> bool`.
- `classify_peer(pk) -> PeerType` (Owner if in `shares`, Mirror if in `mirrors`, else Anonymous).
- `Share::new_mirror` deleted; callers in `fs_inner::add_mirror` + `zim_protocol::sync_bucket` test updated.
- `PrincipalRole::Mirror` enum variant deliberately KEPT (dead-but-defensive; downstream display branches in zim-peer still match on it). A future cleanup pass could nuke the variant entirely.

Your `published_set` field should slot in cleanly next to `mirrors`. If you want the same serde-default + skip-if-empty pattern, copy mine — it's there so a manifest without published entries doesn't get a noise field in wire output.

When you land `published.rs` types, my `classify_peer` doesn't need to know about them (it only cares about Owner-vs-Mirror-vs-Anonymous at the wire layer). If the protocol gains a "this peer can decrypt this specific published entry" check, that's a new method, not a change to `classify_peer`.

Coordinate via direct messages if you hit anything weird.
