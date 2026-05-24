---
title: Local Development
order: 1
---

Build and run Zim from a clone. Five minutes from `git clone` to a daemon and the web hub running side-by-side.

## Prerequisites

- **Rust** 1.75+ — install via [rustup](https://rustup.rs/).
- **System libraries** — see [Install]({{ '/docs/install/' | relative_url }}) for OS-specific package lists (OpenSSL, SQLite, optional FUSE).

## Clone and build

```bash
git clone https://github.com/zim/zim.git
cd zim
cargo build --workspace
```

The first build pulls dependencies and compiles all six crates (`zim-crypto`, `zim-store`, `zim-fs`, `zim-protocol`, `zim-peer`, `zim-hub`). Subsequent builds are incremental.

## Run the daemon (`zim-peer`)

```bash
cargo run -p zim-peer -- init     # one-time, creates state dir and identity
cargo run -p zim-peer -- daemon   # starts the daemon
```

Default ports:

- HTTP API: `http://localhost:3000`
- Local web UI: `http://localhost:8080`

The daemon stays in the foreground; open a second terminal for CLI commands:

```bash
cargo run -p zim-peer -- bucket create my-bucket
cargo run -p zim-peer -- bucket ls my-bucket
```

## Run the web hub (`zim-hub`)

The hub is the read-only public mirror. Run it in a separate terminal:

```bash
make hub
```

This watches `crates/zim-hub/` (Rust, templates, static assets) and auto-reloads via `cargo-watch`. Default URL: `http://localhost:8080/` (override with `HUB_PORT=8090 make hub`). Fallback if you don't have `cargo-watch` installed: `cargo run -p zim-hub`.

Copy `.env.example` → `.env` to set env vars locally; the Makefile target picks them up.

> Note: the daemon's local web UI also defaults to `:8080`. If you run both at once, point one of them at a different port (see env vars below).

## Useful environment variables

| Variable | Default | What it does |
|----------|---------|--------------|
| `ZIM_HUB_LISTEN` | `127.0.0.1:8080` | Listen address for `zim-hub`. |
| `RUST_LOG` | (unset, defaults to `info`) | `tracing` filter. Try `RUST_LOG=zim_hub=debug,tower_http=debug` for the hub or `RUST_LOG=zim_peer=debug` for the daemon. |

Example: run the hub on a different port with verbose logs:

```bash
ZIM_HUB_LISTEN=127.0.0.1:8090 RUST_LOG=zim_hub=debug cargo run -p zim-hub
```

## Hot reload

Install [cargo-watch](https://crates.io/crates/cargo-watch) for auto-rebuild on file changes:

```bash
cargo install cargo-watch
cargo watch -w crates/zim-hub -x 'run -p zim-hub'
```

The watcher rebuilds and restarts on edits to `crates/zim-hub/**` (Rust sources, Askama templates, `static/`).

## Verifying changes

Quick checks before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All four must be green.

## Next

- [Quickstart]({{ '/docs/quickstart/' | relative_url }}) — end-user walkthrough of creating a bucket from a release build.
- [Install]({{ '/docs/install/' | relative_url }}) — system-package install (no source checkout).
