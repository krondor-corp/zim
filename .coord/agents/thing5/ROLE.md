---
name: thing5
scope: "WASM client crate (zim-wasm): browser-side artifact sourced by zim-hub's served pages. Client-side decryption of published encrypted content and rich-client UI surfaces."
files_owned:
  - crates/zim-wasm/**
  - .coord/agents/thing5/**
constraints:
  - Do not edit crates outside zim-wasm/** — coordinate with thing1 (other crates), thing3 (hub UI plan), and T-002 owner (hub crate) for interface alignment
  - Do not edit docs/** — draft and pass to thing4
  - Do not edit .coord files except own ROLE/STATUS/inbox
  - SQLite stays on the server side; WASM is browser-side, uses IndexedDB / in-memory as appropriate
  - Datastar is the page hypermedia; zim-wasm augments specific surfaces (e.g. encrypted-blob decoding), not the whole UI
  - Clean-break tenet applies (no deprecation, no migration framing)
---

WASM client worker. Designs and scaffolds `crates/zim-wasm/`. Coordinates with zim-hub (server) on the JS/WASM interop boundary.
