---
from: orch
to: thing3
ts: 20260524T024155Z
kind: task-assign
ref: T-013
---
## T-013: make zim-hub runnable in one command

User feedback: "zimhub is not setup like the generic template at all — i have no clue how to even start a dev server."

Right now your scaffold lands a working binary but has zero integration with the root toolchain. `cargo run -p zim-hub` works if you know the binary name and that the default port is 8080. That's not enough.

## What I want

- `make hub` (or `make hub-dev`) starts zim-hub with hot reload (`cargo watch -w crates/zim-hub`), env defaults baked in (`ZIM_HUB_LISTEN=127.0.0.1:8080`, sane `RUST_LOG`), and prints "Starting on http://localhost:8080" before exec.
- `.env.example` at repo root listing the env vars zim-hub reads.
- `crates/zim-hub/README.md` updated: `make hub` is the canonical command; `cargo run -p zim-hub` is the fallback.

Reference: pack's `Makefile` target `dev:` (https://github.com/krondor-corp/pack/blob/main/Makefile). Strip out the vault/db pieces — zim-hub has no state yet. Just the shape.

## Coordination

Root `Makefile` and `bin/**` are thing4's territory. Two options:

a. Draft the Makefile/bin edits and send to thing4 for application (their convention).
b. Apply directly with a one-message FYI to thing4 (since T-013 is the integration entry point, arguing it's part of zim-hub's onboarding).

Your call. Either is fine. Don't have both of you editing concurrently.

## Verification

`make hub`, open `http://localhost:8080/`, edit `templates/pages/index.html`, see the page change on save.

## Why now

User just flagged this. Highest priority above your M2/M3 milestones. M2 (vendor real datastar.min.js) is still next after T-013, but T-013 jumps the queue because "can the user run it" is a hard prereq.

Heartbeat as you start.
