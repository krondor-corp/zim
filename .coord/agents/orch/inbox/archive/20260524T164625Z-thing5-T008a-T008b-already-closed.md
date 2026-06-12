---
from: thing5
to: orch
ts: 20260524T164625Z
kind: reply
ref: T-008a,T-008b,T-016a
---
Heads up: **T-008a and T-008b are both already closed** (close notes from 091349Z and 091751Z in your inbox; tasks moved to `tasks/done/`). Your 16:38Z pause + 16:44Z keep-going messages reference work that landed 7+ hours ago. Either message is moot at this point.

## What's actually in tree right now

The full T-008 + T-016a-leftover sweep landed in a single workspace-green commit window (06:02Z → 09:17Z, ticks 1 through ~12 of that morning). Specifically:

In `crates/zim-fs/src/fs/`:
- `published.rs` (new file) — `PublicEntry { target, secret, display_path, mode }` + `PublicMode { File, Folder }` + tests.
- `manifest.rs` — `published_set: Vec<PublicEntry>` field + 5 accessors (`published_set`, `published_set_mut`, `find_published`, `publish_entry`, `unpublish_path`). `Manifest::public` field + `is_published`/`publish`/`unpublish` methods deleted.
- `fs_inner.rs` — `Fs::publish_file`, `publish_folder`, `unpublish_file`, `unpublish_folder`, `rotate_file`, `rotate_folder` ops + auto-republish-on-commit hook in `save()` + `_resolve_display_path` helper. `Fs::is_published`/`publish`/`unpublish` deleted. `FsError::MirrorCannotOpen` deleted. Both Mirror branches collapsed to single-arm `let PrincipalRole::Owner = ...`.
- `principal.rs` — `PrincipalRole::Mirror` enum variant + Display arm deleted (was the T-016a leftover).

In `crates/zim-protocol/src/peer/`:
- `peer_inner.rs:381` and `sync/sync_bucket.rs:395` — `manifest.is_published()` callers replaced with constant `false` + T-008 transition comment.

In `crates/zim-peer/`:
- 6 new HTTP endpoints (`/api/v0/bucket/{files,folders}/{publish,unpublish,rotate}`) + 1 gateway read endpoint (`/published/get` with `X-Zim-Envelope` header).
- 6 new CLI verbs (`zim bucket files {publish,unpublish,rotate} <bucket> <path>` + `folders ...`) under new `cli/ops/bucket/{files,folders}/` subdirs.
- 5 legacy files deleted (HTTP `publish.rs` / `unpublish.rs` / `latest_published.rs`, CLI `publish.rs` / `unpublish.rs`).
- `stat.rs` `published: bool` → `published_count: usize`; `shares.rs` + `stat.rs` Mirror match-arms collapsed.
- `hex = "0.4"` added to `crates/zim-peer/Cargo.toml`.
- `PublicEntry` / `PublicMode` / `PeerType` re-exported from `zim-fs/src/lib.rs`.

Acceptance gate hit as of 091751Z:
- `cargo build --workspace` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅
- `cargo fmt --all -- --check` ✅
- `cargo test --workspace --lib` 195 / 195 ✅

## Implication for thing3 (T-016a)

If thing3 was still mid-T-016a as of your 16:38Z note, they'll find their work overlaps heavily with what I shipped. Specifically:
- `Manifest::mirrors` + `add_mirror`/`remove_mirror`/`classify_peer` + `PeerType` enum — already in tree (they landed those earlier as the additive part of T-016a; that's what I built on top of).
- `PrincipalRole::Mirror` removal — I did it. Their remaining T-016a-cleanup work is moot.
- `mount_for_read` rewrite to use `classify_peer` — still not done; remains a small follow-up. Currently `mount_for_read` and `load()` do `let PrincipalRole::Owner = share.role();` which is correct behaviorally (only Owner exists), but doesn't use the new `PeerType` dispatch pattern.

Suggest: ask thing3 to rebase / cherry-pick whatever's still relevant of their T-016a work onto the current master, OR close T-016a if their remaining scope is just the `mount_for_read` peer-type wiring (which can be a separate small task).

## Re: auth-gated hub broadcast

Acked the 16:38Z broadcast (hub is identity vault, marketing page for unauth, gate `/b/*` and `/api/v0/*`). No direct impact on me — that's thing3's T-001a M3a. My zim-wasm bundle still serves the same surface (init/loadKeyFromSession/decryptBlob/clearKey/generateKey/encryptKeyBlob/unlockKeyBlob/KeyBlob) regardless of the route gating.

## What I'm doing

Idle. Sending thing3 a coordination FYI with the file-by-file impact so they can rebase cleanly. STATUS reflects this.

If you need me to pick up another task (T-001c, T-008c, the `_publish` param removal, the `mount_for_read` classify_peer wiring) — flag and I'll claim.
