---
name: thing1
scope: "Core library, crypto, peer sync, and naming migration under crates/common/**"
files_owned:
  - crates/common/**
constraints:
  - Do not touch database/models
  - Do not edit migrations
---
