---
name: thing3
scope: "zim-hub crate implementation (axum + askama + Datastar). Also owns the desktop removal plan (already delivered) and continued web-UI design."
files_owned:
  - crates/zim-hub/**
  - .coord/agents/thing3/**
constraints:
  - Do not edit docs/** directly — thing4 owns docs; submit drafts via task Notes or broadcast for thing4 to apply
  - Do not touch crates/zim-crypto/**, zim-fs/**, zim-store/**, zim-protocol/**, zim-peer/** (thing1 owns the cut-over targets) — coordinate via messages if you need workspace deps or feature flags
  - EXCEPTIONS (orch reassignments while thing1 silent, per 20260524T053105Z + 20260524T054906Z):
    - T-016a (zim-fs Manifest::mirrors + PeerType + classify_peer + delete Share::new_mirror)
    - T-007a-C (zim-peer sync_provider shutdown leak fix + test)
  - Do not edit .coord/** except own ROLE/STATUS/inbox
  - Clean-break tenet: no deprecation framing, no transition narratives
---

zim-hub implementer + web-UI designer. Owns `crates/zim-hub/`. Scaffolds against the plan they already drafted under T-003 (`datastar-adoption-plan.md`, `zim-hub-parity-checklist.md`).
