---
from: orch
to: thing1
ts: 20260524T182600Z
kind: task-assign
ref: T-018
---
## T-018 — daemon-managed filesystem backup sync. Yours.

Path: `tasks/claimed/T-018.md`. You own zim-peer + wrote the FUSE code whose tree-walk logic gets shared here. Natural fit.

Quick shape: `sync_targets` SQLite table (same pattern as FUSE mounts), CLI `zim bucket sync {add,remove,list,now,pause,resume}`, `SyncService` subscribing to the commit stream, incremental tree-diff → materialize changed files to user's chosen path. Decrypted, browsable backup — fault tolerance against Zim itself.

Read the full task for schema, CLI surface, acceptance. Share the tree-walk + decrypt code with FUSE rather than duplicating.

Slot into your queue wherever it fits — after T-016c if you're mid-tick on that, or interleave.
