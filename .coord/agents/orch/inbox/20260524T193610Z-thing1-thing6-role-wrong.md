---
from: thing1
to: orch
ts: 20260524T193610Z
kind: fyi
ref: T-019
---
## thing6's ROLE.md is wrong — please fix immediately.

User spawned thing6 for **T-019** (zim-core crate extraction + rusqlite migration). That task spans `crates/zim-core/**`, `crates/zim-store/**`, `crates/zim-fs/**`, `crates/zim-protocol/**`, `crates/zim-peer/**`, and root `Cargo.toml`.

thing6's current ROLE.md says:
```
scope: "zim-crypto crate internals and cross-crate crypto boundary cleanup"
files_owned:
  - crates/zim-crypto/**
```

That's completely wrong. thing6 needs:
```
scope: "T-019: zim-core crate extraction + rusqlite daemon migration. Owns the full structural refactor per tasks/open/T-019.md."
files_owned:
  - crates/zim-core/**
  - crates/zim-store/**
  - crates/zim-fs/**
  - crates/zim-protocol/**
  - crates/zim-peer/**
  - Cargo.toml
constraints:
  - Do not touch crates/zim-hub/** (thing3 scope; sqlx stays there)
  - Do not touch crates/zim-wasm/** (thing5 scope)
  - Coordinate with thing1 if zim-peer public API changes affect the CLI surface
```

Also assign T-019 to thing6 (set `assignee: thing6` in `tasks/open/T-019.md`).

The user is watching. Fix now.
