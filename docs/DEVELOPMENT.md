# Development

User-facing local development guide (build, run daemon, run hub, hot reload) lives in the wiki: [Local Development](../wiki/_docs/local-development.md).

This file covers contributor-specific multi-peer dev tooling that doesn't belong in end-user docs.

## Multi-peer dev environment (`make dev`)

The `bin/dev` script creates a multi-node P2P network in tmux with auto-reload for testing sync behaviour.

### Start

```bash
make dev
```

This:

1. Initializes nodes in `./data/node1`, `./data/node2`, etc. (from `bin/dev_/nodes.toml`).
2. Creates a tmux session named `zim-dev`.
3. Starts each node with `cargo-watch` auto-reload.
4. Applies fixtures from `bin/dev_/fixtures.toml`.

### Tmux layout

The `zim-dev` session has three windows:

**Window 0: `zim-nodes`** — one pane per node, each with `cargo watch` running the daemon. Ports from `bin/dev_/nodes.toml`.

**Window 1: `db`** — database inspection. Use `./bin/db node1` to open a node's SQLite.

**Window 2: `api`** — curl examples for each node's API.

### Tmux navigation

```bash
Ctrl+b 0/1/2       # Switch windows
Ctrl+b Left/Right   # Switch panes in window 0
Ctrl+b [            # Scroll mode (q to exit)
Ctrl+b d            # Detach (session keeps running)
tmux attach -t zim-dev  # Reattach
tmux kill-session -t zim-dev  # Kill
```

### Testing P2P sync

With nodes running:

```bash
# Get node IDs
curl http://localhost:3000/api/node/info | jq .node_id
curl http://localhost:3001/api/node/info | jq .node_id

# Create a bucket on node1
curl -X POST http://localhost:3000/api/buckets \
  -H "Content-Type: application/json" \
  -d '{"name": "test-bucket"}'

# Share with node2
curl -X POST http://localhost:3000/api/buckets/{bucket-id}/share \
  -H "Content-Type: application/json" \
  -d '{"peer_id": "NODE2_ID"}'

# Verify sync on node2
curl http://localhost:3001/api/buckets
```

### MinIO (S3-compatible blob storage)

For nodes configured with `blob_store = "s3"`:

```bash
./bin/minio up       # Start MinIO (localhost:9000, console at :9001)
./bin/minio down     # Stop
./bin/minio status   # Check
```

MinIO credentials: `minioadmin:minioadmin`. Bucket: `zim-blobs`.

### Fixtures

`bin/dev_/fixtures.toml` defines initial buckets, files, and shares applied on `make dev` startup. Edit to add test data.

### Database inspection

```bash
./bin/db node1              # Open node1's SQLite
./bin/db node2 "SELECT ..."  # Run a query
```

### Cleanup

```bash
./bin/dev clean       # Remove all dev data (./data/*)
./bin/dev kill        # Kill the tmux session
./bin/dev kill --force # Also kill orphaned port processes
./bin/cleanup --all   # Nuke cargo target + dev data + minio
```
