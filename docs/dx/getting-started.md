# Getting Started

Quick-start for contributors and AI agents.

## Prerequisites

- Rust toolchain (stable)
- [confit](https://confit.krondor.org) >= 0.5 (dev port map; `bin/dev` and `bin/minio` require it):
  `curl -fsSL https://raw.githubusercontent.com/krondor-corp/confit/main/install.sh | bash`
- SQLite 3
- tmux, cargo-watch (for multi-node dev)
- trunk (`cargo install trunk`, for the hub's web SPA)
- docker or podman (for the hub's S3 blob store)

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
cargo run --bin zim -- daemon run

# Start zim-hub (gateway/relay)
make hub

# Whole dev stack in tmux: 2 daemons + hub + minio + fixtures + enroll
make dev

# This worktree's dev ports (each worktree gets its own; see local.md)
./bin/dev ports
```

## Project Layout

See [architecture](../architecture/index.md) for dependency direction and subsystem boundaries.

```
crates/
├── zim-crypto/    # Ed25519/X25519 keys, ChaCha20-Poly1305, secret sharing
├── zim-did/       # DIDs: did:key + did:web, documents, resolver trait
├── zim-core/      # Core: the vault data model — filesystem, content store, linked data. Iroh-free, wasm-clean
├── zim-api/       # Shared HTTP contract + typed client (daemon RPC + hub routes + JWT)
├── zim-peer/      # Peer machinery: sync coordinator, blob/log/contact stores, iroh transport, runtime
├── zim/           # Daemon binary `zim` + CLI + HTTP API + FUSE mounts (feature `fuse`)
└── zim-hub/       # Hub server: ciphertext mirror + did:web identity (+ wasm/ browser SDK, web/ Yew SPA)
```

## Next Steps

- [product/vaults.md](../product/vaults.md) — product overview
- [architecture/vaults/](../architecture/vaults/index.md) — vault implementation
- [patterns/conventions.md](../patterns/conventions.md) — coding conventions
- [contributing.md](contributing.md) — contribution workflow
- [patterns/http-api.md](../patterns/http-api.md) — HTTP API reference
- [patterns/cli.md](../patterns/cli.md) — CLI commands
