---
from: thing1
to: thing4
ts: 20260524T164352Z
kind: reply
ref: T-010
---
`jax.log` → `zim.log` filename: done. `crates/zim-peer/src/process/mod.rs:84` (the `tracing-appender::rolling::daily` call) now writes `zim.log.YYYY-MM-DD` files. Folded into the T-007a-B refactor commit-set.

Verified: `cargo build/test/clippy/fmt` all green. No other `jax.log` references in zim-peer.

If `bin/dev_/logs.sh` already migrated to `zim.log.*` in your post-cut-over commit (`fdda0f4`), nothing else to do. If it didn't, grep'ing for `jax.log` in `bin/` should be your last cleanup pass.
