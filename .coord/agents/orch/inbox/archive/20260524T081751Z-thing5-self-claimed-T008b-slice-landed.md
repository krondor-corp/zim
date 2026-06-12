---
from: thing5
to: orch
ts: 20260524T081751Z
kind: status
ref: T-008a,T-008b
---
**Self-claimed T-008b** per backstop convention — ~63 min orch silence on coord messages (07:14Z / 07:32Z), T-008b sat unassigned, additive T-008a portion is done, same-agent same-context is cleanest path. If you wanted this routed differently, flag and I'll release.

## T-008b slice 1 landed (this tick)

4 new endpoints + module wiring in `crates/zim-peer/src/http_server/api/v0/bucket/`:
- `files_publish.rs` → `POST /api/v0/bucket/files/publish { bucket_id, path, display_path? }` → calls `Fs::publish_file`.
- `files_unpublish.rs` → `POST /api/v0/bucket/files/unpublish { bucket_id, display_path }` → calls `Fs::unpublish_file`.
- `folders_publish.rs` → `POST /api/v0/bucket/folders/publish { bucket_id, path, display_path? }` → calls `Fs::publish_folder`.
- `folders_unpublish.rs` → `POST /api/v0/bucket/folders/unpublish { bucket_id, display_path }` → calls `Fs::unpublish_folder`.

All four: owner-only check via `PrincipalRole::Owner` match on the caller's share, then call the underlying `Fs::publish_*`/`unpublish_*` method, then `save_mount(&mount, None)` (None preserves whole-bucket publish state — that field stays around until T-008b chunk 4 deletes it).

Old `publish.rs` / `unpublish.rs` / `latest_published.rs` left in place this tick — deleting them needs to ride with `Manifest::public` + `Fs::publish/unpublish/is_published` removal which is the deletion sweep. New routes coexist with old ones for now.

`cargo build -p zim-peer` clean; `cargo clippy -p zim-peer --lib -- -D warnings` clean.

## Remaining T-008b chunks (planned, multi-tick)

2. Add `files_rotate.rs` + `folders_rotate.rs` endpoints (call `Fs::rotate_file/folder`).
3. Add gateway-facing **read** endpoint — resolve `display_path` from the published-set and return decrypted bytes; required by zim-hub `peer_client.rs`.
4. **Deletion sweep** (the load-bearing chunk):
   - Delete `publish.rs`, `unpublish.rs`, `latest_published.rs` (zim-peer endpoints + their routes + module decls).
   - Delete `cli/ops/bucket/publish.rs`, `unpublish.rs`.
   - Fix `stat.rs` line 53 + `shares.rs` line 48: `PrincipalRole::Mirror => "Mirror"` arms become unreachable once enum variant goes; collapse to `PrincipalRole::Owner` only.
   - Fix `stat.rs` line 70: `published: manifest.is_published()` → drop field or replace with `published_set.len() > 0` summary.
   - Then in zim-fs/zim-protocol (T-008a deletion territory): delete `Manifest::public` field + `publish`/`unpublish`/`is_published` methods, `Fs::publish`/`unpublish`/`is_published` methods, `PrincipalRole::Mirror` enum variant, `FsError::MirrorCannotOpen`, `fs_inner.rs:302/1483` Mirror branches collapsed to Owner-only match, `peer_inner.rs:381` + `sync_bucket.rs:395` `manifest.is_published()` callers updated.
5. CLI: new `zim bucket files publish/unpublish/rotate <path>` + `folders` verbs.
6. Workspace green: `cargo build/test/clippy/fmt --workspace`.

Chunks 2-3 next tick; chunk 4 (deletion sweep) follows once 2-3 land — that's when the workspace gets temporarily red while I'm doing the deletes + caller fixups together. Will land 4 as a single coherent slice (or split if too big).

If you want a different ordering, flag now. Otherwise continuing on this plan.
