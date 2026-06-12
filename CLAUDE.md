# Project Guide

zim: end-to-end encrypted, peer-to-peer storage built on iroh-blobs with ChaCha20-Poly1305 encryption and X25519 secret sharing.

## Quick Reference

```bash
cargo build                  # Build all crates
cargo test                   # Run all tests
cargo clippy -- -D warnings  # Lint (warnings are errors)
cargo fmt                    # Format code
cargo fmt -- --check         # Check formatting
make hub                     # Start zim-hub dev server with hot reload
make dev                     # Start 2-node dev environment in tmux
cargo run --bin zim -- --help # Run the CLI
```

## Project Structure

```
crates/
├── zim-crypto/    # Ed25519/X25519 keys, ChaCha20-Poly1305, secret sharing
├── zim-core/      # Core: filesystem, content store, linked data, iroh abstraction
├── zim-protocol/  # Wire protocol: peer messaging, sync jobs, append-only bucket log
├── zim-peer/      # System daemon binary `zim` + HTTP API + FUSE + database
├── zim-hub/       # Read-only web mirror gateway (Askama + Datastar)
└── zim-wasm/      # Browser-side WASM client for the hub

docs/              # Contributor/agent documentation
wiki/              # End-user documentation (Jekyll)
bin/               # Dev scripts (dev, check, build, test, db, minio)
```

## Documentation

- `docs/index.md` — Documentation hub and navigation
- `docs/getting-started.md` — Build, run, orient
- `docs/concepts/` — What and why: overview, data model, crypto, sync, security, access model, identity
- `docs/architecture/` — How it's built: project layout, FUSE
- `docs/reference/` — Lookup: HTTP API, CLI, debugging, dev environment
- `docs/deployment/` — Ship: release process, infra (future)
- `docs/PATTERNS.md` — Error handling, async, serialization, module org
- `docs/CONTRIBUTING.md` — Contribution workflow, commit conventions, test readability
- `docs/CRATES.md` — Crate layout, dependency graph, naming conventions
- `docs/SUCCESS_CRITERIA.md` — CI checks that must pass
- `docs/ISSUES.md` — Issue and ticket conventions

## Audience split (binding)

- **`wiki/`** — end-user facing. Operational, copy-pasteable. No Rust internals, no codebase paths, no struct dumps.
- **`docs/`** — contributors and AI agents. Architecture, patterns, processes, internals.

## Constraints

1. **All CI checks must pass** before creating a PR:
   - `cargo build` — must compile
   - `cargo test` — all tests pass
   - `cargo clippy -- -D warnings` — no warnings
   - `cargo fmt -- --check` — code formatted
2. **Follow existing patterns** — match style of existing code (see `docs/PATTERNS.md`)
3. **Write tests** — unit tests in `#[cfg(test)]` modules, integration tests in `tests/`
4. **Tests must read like stories** — named actors (Alice, Bob), scenario-based names, clear section comments
5. **Update docs** — keep `docs/` in sync with code changes
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
