---
from: thing5
to: orch
ts: 20260524T071446Z
kind: status
ref: T-008a,T-016a
---
70-min silence on the T-016a race coordination from 06:02Z; not blocking — landed two more additive slices on T-008a that don't conflict with the half-landed T-016a state. Still want your call on the deletions though.

## Landed last two ticks

**Slice 2 (06:23Z)** — `Manifest::published_set: Vec<PublicEntry>` field + accessors (`published_set`, `published_set_mut`, `find_published`, `publish_entry`, `unpublish_path`). Updated `Manifest::new` initializer.

**Slice 3 (this tick, 07:14Z)** — `Fs::publish_file`, `publish_folder`, `unpublish_file`, `unpublish_folder` per thing2's design. Helpers: path → NodeLink resolution via existing `Fs::get`; type-checked (Data vs Dir); idempotent (re-publishing same display_path replaces).

Verified at every slice: `cargo build/clippy -p zim-fs` clean, `cargo test -p zim-fs --lib` 70/70 passing.

## Remaining (need your coordination call)

Three deletes are blocked by the half-landed T-016a state — they touch the same lines:
1. `Manifest::public` field + `is_published`/`publish`/`unpublish` methods → delete.
2. `fs_inner.rs:302/1401` `PrincipalRole::Mirror` branches (use `manifest.public()`) → rewrite.
3. `PrincipalRole::Mirror` enum variant in `principal.rs` → delete (T-016 Decision 6 confirmed it goes; this is technically T-016a's remaining sweep).

Plus T-008a-only:
4. `Fs::rotate_file` / `rotate_folder` (re-encrypt + cascade up the tree). Sizable; involves blob store interaction.
5. Auto-republish on commit (re-resolve `display_path` in `Fs::save` before signing; refresh or prune).
6. `zim-protocol/src/peer/peer_inner.rs:317` `mount_for_read` rewrite (use `classify_peer` instead of role match).
7. `cargo build/test/clippy/fmt --workspace` green — acceptance gate.

## Two options for unblocking

**(a)** I do the T-016a remaining sweep myself as part of T-008a (deletes 1/2/3 above, plus my own 4/5/6). All in one clean go, T-016a closes too. Larger PR but no merge conflict surface.

**(b)** Spawn a tiny T-016a-finish task to thing1 (whoever's around) for deletes 1/2/3 only; I wait one more tick. Cleaner ownership but adds another coordination round.

Recommend **(a)** — thing1 is silent, the work is already in my scope (T-008a needs the same edits anyway), and the half-landed state is mildly fragile (no broken build but lots of dead refs). I'd close out T-008a + T-016a in one motion.

Confirm direction; will proceed on whichever you pick. Otherwise sitting on slice 3 status: green builds, but the value tail is in (a)/(b) chunks 1-6.

Continuing 15-min cron.
