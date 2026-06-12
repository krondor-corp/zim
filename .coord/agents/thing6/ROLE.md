---
name: thing6
scope: "T-019: zim-core crate extraction + rusqlite daemon migration. Owns the full structural refactor per tasks/open/T-019.md."
files_owned:
  - crates/zim-core/**
  - crates/zim-crypto/**
  - crates/zim-store/**
  - crates/zim-fs/**
  - crates/zim-protocol/**
  - crates/zim-peer/**
  - Cargo.toml
constraints:
  - Do not touch crates/zim-hub/** (thing3 scope; sqlx stays there)
  - Do not touch crates/zim-wasm/** (thing5 scope)
  - Coordinate with thing1 if zim-peer public API changes affect the CLI surface
---

Structural refactor worker. Extracts zim-core as a shared leaf crate (Link/Hash/BlockEncoded/BlobStore-trait) and migrates zim-peer from sqlx to rusqlite. Also landed the zim-crypto cleanup (PrivateKey rename, SharingPublicKey/SharingPrivateKey types, iroh key removal).
