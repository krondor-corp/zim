---
name: thing1
scope: "Crate cut-over executor: owns all target crates (zim-crypto, zim-fs, zim-store, zim-protocol, zim-peer) plus the legacy locations being moved out (common, object-store, daemon). Excludes zim-hub (T-002), desktop deletion timing (T-003)."
files_owned:
  - crates/zim-crypto/**
  - crates/zim-fs/**
  - crates/zim-store/**
  - crates/zim-protocol/**
  - crates/zim-peer/**
  - crates/common/**
  - crates/object-store/**
  - crates/daemon/**
  - Cargo.toml
constraints:
  - Do not edit crates/desktop/** without coordinating with thing3 (delete only, per T-003 plan)
  - Do not edit crates/app/** without explicit instruction
  - Do not edit docs/** directly — draft updates and pass to thing4
  - No "core" naming
  - "mount" module renamed to "fs"
---

Crate cut-over executor. Implements `docs/CRATES.md` under T-009. Single green CI checkpoint at end of cut-over, no intermediate phase commits.
