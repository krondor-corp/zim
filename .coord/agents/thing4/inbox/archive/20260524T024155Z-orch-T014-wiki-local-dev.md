---
from: orch
to: thing4
ts: 20260524T024155Z
kind: task-assign
ref: T-014
---
## T-014: wiki page for local development

User wants a `wiki/_docs/local-development.md` page that tells a user how to run Zim locally. Operational, hands-on. Read it in 5 minutes and have something running.

## Hard constraints from user

- **NO big sweeping architecture docs.** Wiki is end-user facing. No "how Zim works" essays, no CRDT theory, no protocol explanations. Architecture stays in `docs/` (contributor-facing).
- **One page.** Not a directory. Not a deck.

## What goes on the page

- Prereqs (Rust toolchain).
- `cargo build --workspace`.
- How to run the daemon (zim-peer, current command + default ports).
- How to run zim-hub: `make hub` once thing3 lands T-013; `cargo run -p zim-hub` as interim if T-013 isn't in yet.
- Common env vars (`ZIM_HUB_LISTEN`, `RUST_LOG`).
- Add a "Development" nav group in `wiki/_data/nav.yml`.

## Ordering vs your other work

1. **First**: finish the overshoot cleanup from `20260524T022500Z-orch-audit-fix-the-overshoot.md` (delete or placeholder the 13 stale wiki pages).
2. **Then**: T-014.

## Coordination with thing3

thing3 has T-013 (the actual `make hub` workflow). If T-013 lands before you draft, use `make hub`. If you draft first, use `cargo run -p zim-hub` and leave a one-line note for yourself to swap when T-013 lands. Don't block.

Heartbeat when overshoot cleanup is done and again when T-014 lands.
