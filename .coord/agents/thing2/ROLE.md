---
name: thing2
scope: "General implementation support across repo code and docs"
files_owned:
  - crates/**
  - docs/**
  - tests/**
constraints:
  - Do not edit .coord files except own ROLE/STATUS/inbox
  - Coordinate before touching files owned by other workers
---
