---
from: thing5
to: orch
ts: 20260524T060249Z
kind: reply
ref: T-008a,T-008
---
## T-008a claimed + first slice landed.

Claimed (moved `tasks/open/T-008a.md` → `tasks/claimed/`, assignee thing5). Surveyed thing2's T-008 proposal in full + current state of `crates/zim-fs/src/fs/` (manifest.rs 581 LOC, fs_inner.rs 1553 LOC, node.rs 279 LOC, path_ops.rs 1059 LOC — full T-008a will span 3–5 ticks).

## Landed this tick

- **New file `crates/zim-fs/src/fs/published.rs`** — `PublicEntry { target, secret, display_path, mode }` + `PublicMode { File, Folder }` per thing2's design verbatim. Helpers: `new_file`, `new_folder`, `matches_display_path` (strict equality, recursive sub-path matching is the walk layer's job).
- **`mod.rs` updated** — `mod published;` + `pub use published::{PublicEntry, PublicMode};`.
- **Two tests** — Alice publishes file, Bob publishes folder; both round-trip through DAG-CBOR and exercise the helper accessors.

Verified:
- `cargo build -p zim-fs` — clean.
- `cargo test -p zim-fs --lib published::` — 2/2 passing.

## Remaining T-008a chunks (planned, will work next ticks)

1. **Add `published_set: Vec<PublicEntry>` field to `Manifest`** + accessors (additive — won't break callers).
2. **Delete `Manifest::public` field + `publish` / `unpublish` / `is_published` methods** — plus downstream callers in `fs_inner.rs` (`PrincipalRole::Mirror` branch at line 302 uses `manifest.public()`).
3. **Add per-node ops to `Fs`**: `publish_file(path)`, `unpublish_file(path)`, `publish_folder(path)`, `unpublish_folder(path)`, `rotate_file(path)`, `rotate_folder(path)`. These need to walk to a node via existing `Fs` path resolution (uses `path_ops` machinery), copy out the `NodeLink::Secret`, append to `published_set`.
4. **Auto-republish on commit** — wire into `Fs::save` (where the manifest gets re-signed): iterate `published_set`, re-resolve `display_path`, refresh `target`+`secret` in place, prune dangling.
5. **Rotate ops** — generate fresh `Secret`, re-encrypt + re-link cascade up to root, refresh entry.
6. **Clean up zim-protocol references** — `peer_inner.rs:317` `mount_for_read` branches on `PrincipalRole::Mirror` + `manifest.public()`; both go away.
7. **`cargo build/test/clippy/fmt --workspace` green** — acceptance gate.

## Coordination flag — race with T-016a

Diagnostics from this tick show `crates/zim-fs/` is in a partial-T-016a state: `PeerType` enum was added to `principal.rs` (re-exported through `mod.rs`), and there are `manifest.rs` / `fs_inner.rs` references to `Share::new_mirror` and `PeerType` that haven't been wired. `cargo build -p zim-fs` succeeded, so master is buildable, but rust-analyzer is flagging in-flight work. Looks like T-016a is partially landed by someone (thing1? you?) but not finished.

If T-016a is in flight by another worker, I'll hit merge conflicts on `manifest.rs` and `fs_inner.rs`. Two options:
- (a) I pause T-008a and wait for T-016a to fully land. Clean dependency order.
- (b) I proceed, and whichever lands second rebases. Both touch the same struct.

Recommend (a) if T-016a is being actively worked. **Confirm**: is anyone actively on T-016a right now, and if so, who? If T-016a is also stalled, I'll proceed and handle the merge.

## Posture

Continuing 15-min cron; STATUS now active on T-008a. Will land the next slice on the next tick unless you redirect or T-016a coordination changes the plan.
