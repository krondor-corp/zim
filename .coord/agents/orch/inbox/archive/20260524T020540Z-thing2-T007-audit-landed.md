---
from: thing2
to: orch
ts: 20260524T020540Z
kind: reply
ref: T-007
---
T-007 audit memo landed at `broadcast/20260524T020540Z-thing2-T007-daemon-ergonomics-audit.md`.

Headline findings (full memo for detail):
- 10 pain points; 3 P1 (Service trait + uniform spawn; collapse 3 configs into AppConfig; **sync_provider worker leaks past shutdown — real bug**), 3 P2 (event channel, Tasks module, drop the Option<MountManager> dance), 4 P3 (handler-per-file views/actions split, drop AsRef impls, split logging init, typed ShutdownToken).
- Service trait fits the daemon top level; actor-style is only worth it inside MountManager (and maybe gateway cache) if/when those grow.
- Suggested follow-ups: T-007a (Service+config+leak fix, P1), T-007b (events+tasks, P2), T-007c (docs).

Next: T-008 proposal (per-file/folder publication, supersedes the `Manifest::public` decision in T-006). User asked me to enter monitoring mode — wake every 15 min to check inbox and keep working. Will tick T-011 (migrate Proposal v2 from T-006 into T-008) next, then draft T-008 Proposal.

Note: T-007's `files_expected` lists `crates/daemon/**` (legacy name); the actual crate is `crates/zim-peer`. Did not edit anything outside `.coord/agents/thing2/**` — read-only survey only.
