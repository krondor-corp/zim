---
title: Local Development
order: 1
---

Build and run Zim from a clone.

## Prerequisites

- **Rust** 1.75+ — install via [rustup](https://rustup.rs/).
- **System libraries** — see [Install]({{ '/docs/install/' | relative_url }}) for OS-specific package lists (OpenSSL, SQLite, optional FUSE).

## Clone and build

```bash
git clone https://github.com/zim/zim.git
cd zim
cargo build --workspace
```

The first build pulls dependencies and compiles all crates (`zim-crypto`, `zim-core`, `zim-core`, `zim-protocol`, `zim-peer`, `zim-hub`, `zim-wasm`). Subsequent builds are incremental.

## Run the web hub (`make hub`)

The hub is a single-process binary: it boots its own HTTP gateway **and** an embedded iroh peer in-process. No separate daemon to run.

```bash
make hub
```

Default URL: `http://localhost:17190/`. The embedded peer keeps its SQLite database and blob store under `./data/zim-hub/` (created on first launch).

`make hub` runs the hub in a tmux window (`zim-dev:hub`) and auto-reloads via `cargo-watch` when the hub or shared sync crates change:

```bash
cargo install cargo-watch  # one-time, if you don't have it
```

If you don't want hot reload, fall back to:

```bash
cargo run -p zim-hub
```

### Configure via `.env`

Copy the template and edit:

```bash
cp .env.example .env
```

The Makefile reads from your shell environment — `source .env` (or use direnv) before running `make hub`.

| Variable | Default | What it does |
|----------|---------|--------------|
| `ZIM_HUB_LISTEN` | `127.0.0.1:17190` | HTTP listen address. |
| `ZIM_HUB_DATA` | `./data/zim-hub` | Where the embedded peer keeps its SQLite DB and blob store. |
| `ZIM_HUB_LOG` | `info` | Hub-only log level. |
| `RUST_LOG` | (unset) | Full `tracing` filter. Overrides `ZIM_HUB_LOG` and the Makefile default. Try `RUST_LOG=info,zim_hub=debug,tower_http=debug`. |
| `HUB_PORT` | `17190` | Convenience override for `ZIM_HUB_LISTEN`'s port (`HUB_PORT=17191 make hub`). |

## Mirror a bucket on zim-hub

zim-hub acts as a **mirror peer** — it holds and serves a bucket's public files without ever holding the bucket's secret. To put a bucket onto a hub, the bucket's owner pre-authorizes the hub's peer key.

### One-time setup per bucket

1. **Start zim-hub.** On first boot it prints its node id and a ready-to-copy command:

   ```
   To mirror a bucket on this hub, run on the owning peer:
     zim bucket mirror add <BUCKET_ID> 1ea75079a6bc194f4b3e28dad40b49c8762ae0832fcba25ff043c1ff7f7ced81
   ```

2. **On the owning peer** (the machine running `zim` as a member of the bucket), substitute the bucket id and run the command:

   ```bash
   zim bucket mirror add <YOUR_BUCKET_ID> <HUB_NODE_ID>
   ```

3. The hub fetches the bucket's public files and surfaces them at `http://localhost:17190/b/<BUCKET_ID>/tree`.

### Stable node id

The hub's node id stays the same across restarts as long as you keep the `data` directory (default `./data/zim-hub/`). The directory holds the iroh secret key.

- **Move the data directory** — the same node id moves with it; mirroring keeps working.
- **Delete the data directory** — new node id on next boot; re-authorize with `zim bucket mirror add` against the new id.

### What gets mirrored

Only files explicitly marked public in the bucket. zim-hub never sees the bucket secret, so private content is never decrypted on the hub side.

### Multi-bucket

Run `zim bucket mirror add` once per bucket you want the hub to serve. The hub aggregates them — `http://localhost:17190/` lists every mirrored bucket.

### Removing a mirror

On the owning peer:

```bash
zim bucket mirror remove <BUCKET_ID> <HUB_NODE_ID>
```

The hub keeps the blobs it already fetched (no auto-eviction in v1) but stops receiving new ones. To free disk space, delete the bucket's contents from the hub's data directory manually.

## Run the headless daemon (`zim-peer`)

You only need this if you want the CLI, FUSE mount, or a separate daemon to drive P2P sync. The hub doesn't need it.

```bash
cargo run -p zim-peer -- init     # one-time, creates state dir and identity
cargo run -p zim-peer -- daemon   # foreground; ctrl-c to stop
```

In another terminal:

```bash
cargo run -p zim-peer -- bucket create my-bucket
cargo run -p zim-peer -- bucket ls my-bucket
```

The daemon's HTTP API listens on `http://localhost:3000` and its local Web UI on `http://localhost:17190`. If you also run the hub, point one of them at a different port (`ZIM_HUB_LISTEN=127.0.0.1:8090 make hub`).

## Multi-peer P2P sync (`make dev`)

For testing sync between peers, `make dev` spawns a two-node tmux environment with hot reload on each:

```bash
make dev
```

Use this when you're working on `zim-protocol` / `zim-core` / sync behaviour. It's not needed for hub UI work.

## Verifying changes

Quick checks before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All must be green.

## Cleanup

To nuke build artefacts (cargo target dir + the Jekyll wiki build cache):

```bash
make cleanup
```

`make cleanup-all` also removes `./data/` (dev node state) and `.minio` (MinIO local).

## Next

- [Quickstart]({{ '/docs/quickstart/' | relative_url }}) — end-user walkthrough of creating a bucket from a release build.
- [Install]({{ '/docs/install/' | relative_url }}) — system-package install (no source checkout).
