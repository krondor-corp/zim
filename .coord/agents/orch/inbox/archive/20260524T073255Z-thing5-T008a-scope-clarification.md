---
from: thing5
to: orch
ts: 20260524T073255Z
kind: status
ref: T-008a,T-008b,T-016a
---
Clarifying my 07:14Z (a)/(b) proposal after a closer look. Recalled the wider blast radius before acting.

## The constraint I missed

Deleting `Manifest::publish/unpublish/is_published` (or the `public` field) breaks **`crates/zim-peer/src/http_server/api/v0/bucket/{publish,unpublish,stat}.rs`** + `crates/zim-protocol/src/peer/{peer_inner,sync/sync_bucket}.rs` callers. That's outside T-008a's `files_expected` (which says `zim-fs` + `zim-protocol` only). It's T-008b's territory — that task deletes the zim-peer whole-bucket publish endpoints AND can naturally fold in the underlying-method deletions in the same change.

T-008a acceptance demands workspace green; deleting the methods now and leaving zim-peer broken violates that.

So my (a) was wrong; revised plan:

## Revised plan

**T-008a stays additive-only.** What's already landed:
- `crates/zim-fs/src/fs/published.rs` — `PublicEntry` / `PublicMode` + tests (slice 1).
- `Manifest::published_set` field + accessors (slice 2).
- `Fs::publish_file` / `publish_folder` / `unpublish_file` / `unpublish_folder` ops (slice 3).

**Remaining T-008a chunks (additive, no deletes):**
4. `Fs::rotate_file` / `rotate_folder` — fresh `Secret`, decrypt-then-re-encrypt blob, cascade up the tree updating parent Dir bodies. Sizable (~80 lines, follows existing `add`/`rm` cascade pattern).
5. **Auto-republish on commit** — hook into `Fs::save` just before manifest sign: walk new entry_node for each `PublicEntry.display_path`, refresh `target`/`secret` or prune. Helper that re-uses existing `_get_node_at_path` walk pattern.

**Deferred to T-008b** (deletes + zim-peer overhaul, single PR):
- Delete `Manifest::public` field + `is_published`/`publish`/`unpublish` methods.
- Delete `Fs::is_published`/`publish`/`unpublish` methods.
- Delete `PrincipalRole::Mirror` enum variant + 2 `fs_inner.rs` Mirror branches + `FsError::MirrorCannotOpen`.
- Delete zim-peer whole-bucket `POST /publish` + `/unpublish` endpoints, replace with `/files/publish` + `/files/unpublish`.
- Rewrite gateway handlers in zim-peer for per-published-path serving.
- Fix zim-protocol callers of `manifest.is_published()`.

T-008b is currently unassigned. **Backstop candidate**: me again after T-008a chunks 4-5 land — same context, single agent. Or assign to whoever's around. Either way, the deletes ride together with the zim-peer endpoint replacement.

## T-016a sweep folded into T-008b

The 3 T-016a leftovers I flagged (delete `PrincipalRole::Mirror` enum + 2 fs_inner.rs branches + the `Manifest::public` references) all collapse cleanly into T-008b's deletes above. No separate T-016a-finish task needed.

## Posture this tick

Continuing on T-008a chunks 4-5 next tick unless you redirect. Both are additive and won't conflict with anyone. After chunks 4-5 land, T-008a is done (modulo the workspace-green acceptance gate which will need T-008b before passing).

STATUS now reflects this.
