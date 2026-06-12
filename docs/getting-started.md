# Getting Started

Quick-start for contributors and AI agents.

## Prerequisites

- Rust toolchain (stable)
- SQLite 3
- tmux (for multi-node dev)

## Build & Verify

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check
```

## Run

```bash
# CLI help
cargo run --bin zim -- --help

# Initialize a local node
cargo run --bin zim -- init

# Start the daemon
cargo run --bin zim -- daemon

# Start zim-hub (gateway/relay)
make hub

# 2-node dev environment (tmux)
make dev
```

## Project Layout

See [architecture/project-layout.md](./architecture/project-layout.md) for the full crate map.

```
crates/
├── zim-crypto/    # Keys, AEAD, secret sharing
├── zim-core/     # Core library: filesystem, content store, linked data
├── zim-protocol/  # Wire protocol: peer sync, bucket log
├── zim-peer/      # System daemon binary + HTTP API + FUSE
├── zim-hub/       # Web gateway (Relay + Mirror + identity vault)
└── zim-wasm/      # Browser-side WASM client
```

## Next Steps

- [concepts/overview.md](./concepts/overview.md) — architecture overview
- [PATTERNS.md](./PATTERNS.md) — coding conventions
- [CONTRIBUTING.md](./CONTRIBUTING.md) — contribution workflow
- [reference/api.md](./reference/api.md) — HTTP API reference
- [reference/cli.md](./reference/cli.md) — CLI commands
