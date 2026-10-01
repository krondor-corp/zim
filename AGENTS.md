# Project Guide

zim: end-to-end encrypted, peer-to-peer storage built on iroh-blobs with ChaCha20-Poly1305 encryption and X25519 secret sharing.

## Quick Reference

```bash
cargo build                  # Build all crates
cargo test                   # Run all tests
cargo clippy -- -D warnings  # Lint (warnings are errors)
cargo fmt                    # Format code
cargo fmt -- --check         # Check formatting
make hub                     # Start the hub in the dev tmux session (hot reload)
make dev                     # Start 2-node dev environment in tmux
make e2e                     # One-shot e2e: clean start, fixtures, sync checks
cargo run --bin zim -- --help # Run the CLI
```

## Project Structure

```
crates/
├── zim-crypto/    # Ed25519/X25519 keys, ChaCha20-Poly1305, secret sharing
├── zim-did/       # DIDs: did:key + did:web, documents, resolver trait
├── zim-core/      # Core: the vault data model — filesystem, content store, linked data. Iroh-free, wasm-clean
├── zim-api/       # Shared HTTP contract + typed client (daemon RPC + hub routes + JWT)
├── zim-peer/      # Peer machinery: sync coordinator, blob/log/contact stores, iroh transport, runtime
├── zim/           # Daemon binary `zim` + CLI + HTTP API + FUSE mounts (feature `fuse`)
├── zim-hub/       # Hub server: ciphertext mirror + did:web identity (+ wasm/ browser SDK, web/ Yew SPA)
└── zim-e2e/       # E2E harness bin: hermetic daemons + fixtures + convergence verdicts (never published)

docs/              # Contributor/agent documentation
web/               # Site: homepage + end-user docs (Jekyll, pack-style)
bin/               # Dev scripts (dev + dev_/ incl. fixtures, hub, minio)
```

## Documentation

- `docs/index.md` — Documentation hub and navigation
- `docs/_guidelines/` — Audience, placement, and public-site rules
- `docs/product/` — Capabilities, guarantees, access, identity, security, roadmap
- `docs/architecture/` — Subsystem boundaries, data relationships, flows, invariants
- `docs/ui/` — Browser interface and WASM boundary
- `docs/patterns/` — Cross-cutting conventions, CLI boundaries, HTTP contracts
- `docs/dx/` — Setup, local development, debugging, and contribution
- `docs/devops/` — Release and operational workflows

## Audience split (binding)

- **`web/`** — end-user facing site (homepage + docs). Operational, copy-pasteable. No Rust internals, no codebase paths, no struct dumps.
- **`docs/`** — contributors and AI agents. Architecture, patterns, processes, internals.

## Constraints

1. **All CI checks must pass** before creating a PR:
   - `cargo build` — must compile
   - `cargo test` — all tests pass
   - `cargo clippy -- -D warnings` — no warnings
   - `cargo fmt -- --check` — code formatted
2. **Follow existing patterns** — match style of existing code (see `docs/patterns/conventions.md`)
3. **Write tests** — unit tests in `#[cfg(test)]` modules, integration tests in `tests/`
4. **Tests must read like stories** — named actors (Alice, Bob), scenario-based names, clear section comments
5. **Update docs** — keep `docs/` in sync with code changes
   - Keep `.claude/skills/` in sync when commands or workflows change
6. **Op pattern for CLI** — commands return typed data, never print; formatting in Display impls
7. **Module per responsibility** — split files > 200 lines with distinct sections

## Do Not

- Push to main directly — create a PR
- Skip clippy warnings — fix them
- Add debug code (`println!`, `dbg!`) to commits
- Use `#[allow(dead_code)]` — remove unused code instead
- Write speculative code without a caller
- Print from Op::execute() — return data, format in Display
- Put infrastructure config as daemon flags — set at init time
- Add `--json` flag to CLI — use the HTTP API for machine output
- Create documentation files unless explicitly asked
