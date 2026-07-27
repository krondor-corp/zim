# Backup targets — continuous vault → folder mirroring

**Stage:** Planned
**Priority:** Low

## Objective

Register local directories as continuous backup targets for a vault:
whenever the vault head advances, materialize the tree as plain files
into each registered target ("Dropbox folder" UX, decrypted at rest by
explicit user choice).

## Prior art (spec, not code)

Archived `_zim-peer`:
- `src/backup_sync.rs` — 30s poll comparing head vs `last_head`; on
  change, full-tree decrypt-to-disk dump + a `.zim-sync` marker file.
- `src/http_server/api/v0/bucket/sync.rs` — CRUD surface:
  `sync/{add,remove,list,pause,resume}` with per-target status
  (Active/Paused/error).
- `src/database/models/sync_target.rs` — persistence (SQLite; live
  convention would be a JSON store like `mount/store.rs`, keyed by
  `VaultId`).

## Successful approach (what we'd do differently)

- Don't poll: subscribe to head-advance (the daemon already knows when
  it saves or pulls — same hook that fires `announce_head` / the mount
  cache invalidation).
- Reuse [`vault export`](vault-export.md) as the materialize
  primitive; v1 full re-dump on change is fine (prior art did the same),
  incremental diff later.
- Registration mirrors the mounts pattern exactly: JSON store, HTTP
  CRUD, `zim backup {add,list,remove,pause,resume}`, auto-resume on
  daemon boot.

Deferred by explicit decision (2026-07); export ships first.

> The archived crate was deleted after the 2026-07 salvage audit — view the cited files via git history (`git log --all -- 'crates/_zim-peer'`).
