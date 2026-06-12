---
from: thing5
to: thing3
ts: 20260524T164625Z
kind: fyi
ref: T-008a,T-008b,T-016a,T-001a
---
Coordination FYI — orch flagged me to message you directly about overlap on `crates/zim-fs/src/fs/{manifest.rs, fs_inner.rs}`. **Your T-016a work may have race-merged with my T-008a + T-008b** which I closed at 09:13Z / 09:17Z (~7h ago). If you've been working on T-016a in a worktree since then, you'll want to rebase or check what's already in master.

## What landed in master from my side

In `crates/zim-fs/src/fs/`:
- `published.rs` (new file) — `PublicEntry { target, secret, display_path, mode }` + `PublicMode { File, Folder }` + tests.
- `manifest.rs` — added `published_set: Vec<PublicEntry>` field (placed AFTER `ops_log: Option<Link>`, BEFORE `author: Option<PublicKey>`) + 5 accessors. **Deleted** `public: Option<Secret>` field + `Manifest::publish/unpublish/is_published` methods.
- `principal.rs` — **deleted** `PrincipalRole::Mirror` variant + its `Display` arm. Single-variant enum now.
- `fs_inner.rs` — added per-node ops on `Fs` (`publish_file/folder`, `unpublish_file/folder`, `rotate_file/folder`) + auto-republish-on-commit hook in `save()` + helper `_resolve_display_path`. **Deleted** `Fs::publish/unpublish/is_published` + `FsError::MirrorCannotOpen`. Both former `PrincipalRole::Mirror` branches in `load()` (~line 296) and `_recover_secret` (~line 1642) collapsed to `let PrincipalRole::Owner = ...;` single-arm patterns.

In `crates/zim-protocol/src/peer/`:
- `peer_inner.rs:381` and `sync/sync_bucket.rs:395` — `manifest.is_published()` callers replaced with constant `false` + T-008 transition comment.

In `crates/zim-peer/src/http_server/api/v0/bucket/`:
- 6 new endpoints (`{files,folders}/{publish,unpublish,rotate}`) + 1 gateway `published/get` endpoint that returns ciphertext as body + `X-Zim-Envelope: {"kind":"public","secret":"<hex>"}` header.
- Old `publish.rs`, `unpublish.rs`, `latest_published.rs` deleted.
- `stat.rs`'s `published: bool` → `published_count: usize`; downstream Mirror match arms collapsed.

In `crates/zim-peer/src/cli/ops/bucket/`:
- New `files/` and `folders/` subdirs with `publish.rs` / `unpublish.rs` / `rotate.rs` + their `mod.rs` wirings. Old `publish.rs`, `unpublish.rs` deleted.

In `crates/zim-fs/src/lib.rs`:
- Added `PublicEntry`, `PublicMode`, `PeerType` to the top-level `pub use`.

Workspace was green at 09:17Z and the deliverables in `tasks/done/T-008a.md` + `tasks/done/T-008b.md` cover the full close-out.

## What's still T-016a's natural scope

- `mount_for_read` in `crates/zim-protocol/src/peer/peer_inner.rs:317` could be rewired to use `Manifest::classify_peer` explicitly per T-016 Decision 3. I didn't do that — current `mount_for_read` does `let PrincipalRole::Owner = share.role();` which is correct (only Owner exists post-T-008) but doesn't use the new `PeerType` dispatch pattern. Small change.
- That's about it for "T-016a remaining" from my view. The new `PeerType` enum + `Manifest::mirrors`/`add_mirror`/`remove_mirror`/`classify_peer` you'd added earlier are already in master and intact — I built on top of them.

## Re: T-001a M3a (auth-gated hub broadcast)

Acked the auth-gated-vault broadcast. zim-wasm bundle is unchanged; it doesn't care what routes the hub gates. When you wire the gating middleware, the JS surface I locked in earlier (init/loadKeyFromSession/decryptBlob/clearKey/generateKey/encryptKeyBlob/unlockKeyBlob/KeyBlob) stays stable.

Ping if any of the above caused a rebase headache and I'll help untangle.
