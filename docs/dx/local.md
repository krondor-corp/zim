# Development

User-facing local development guide (build, run daemon, run hub, hot reload) lives in the site docs: [Local Development](../../web/_docs/local-development.md).

This file covers contributor-specific multi-peer dev tooling that doesn't belong in end-user docs.

## Multi-peer dev environment (`bin/dev`)

`bin/dev` spins up a multi-node P2P network in tmux so you can exercise sync, sharing, mounting, and the hub locally. It builds the workspace's debug binary once (`cargo build -p zim` → `target/debug/zim`) and runs that same binary for every peer — **not** `cargo run`, and **not** an installed `~/.cargo/bin/zim`. Peers are isolated purely by `ZIM_HOME`.

Nodes are defined in `bin/dev_/nodes.toml`. The section name is the peer's nick (`alice`, `bob`), its data dir (`data/<nick>/`), its tmux pane label, and the name of the generated shell function. Default daemon port is `17171`; dev ports start at `17172`.

```toml
[alice]
api_port = 17172

[bob]
api_port = 17173
```

### Commands

```bash
./bin/dev                       # = run; build + start daemons in tmux (attaches)
./bin/dev run -b|--background   # start detached
./bin/dev run --fuse            # build daemons with the `fuse` feature (zim mount)
./bin/dev run --hub             # full stack: daemons + hub + fixtures + enroll
./bin/dev --hub                 # shorthand for `run --hub`
./bin/dev status                # which daemons answer /_status/livez
./bin/dev cli <nick> <args...>  # run `zim` against one node (sets its ZIM_HOME)
./bin/dev shell                 # emit `alice`/`bob` shell functions (see below)
./bin/dev logs <nick>           # dump that node's tmux pane
./bin/dev seed                  # create vaults + cross-shares (daemon-only)
./bin/dev hub [up|enroll|did|down]
./bin/dev clean                 # rm -rf each node's data dir
./bin/dev kill [-f|--force]     # kill the tmux session; -f also frees ports
```

`make dev` is just `./bin/dev`, and subcommand words pass straight through: `make dev hub up`, `make dev status`, `make dev cli alice vault list`. Flags need the `ARGS` form (`make dev run ARGS="-b --fuse"`) because make claims leading dashes for itself; the "overriding commands" warning on colliding words (clean, hub, e2e) is expected and harmless.

`make e2e` (= `./bin/dev e2e`) is the one-shot test run: clean start → daemons → fixtures → cross-node sync checks, exiting nonzero on failure. Peers are cross-introduced with direct NodeAddrs (`/api/v0/peers/{addr,introduce}`), so local dials skip DHT discovery — runs are hermetic and converge in seconds. Sync assertions poll until converged or `E2E_DEADLINE` (default 60s). The seeded environment is left running.

### Per-peer shells

`./bin/dev shell` prints one function per node that wraps the binary with the right `ZIM_HOME`:

```bash
eval "$(./bin/dev shell)"
alice vault list
bob vault cat demo /readme.md
```

### Tmux layout

The `zim-dev` session has:

- **Window 0 `nodes`** — one **pane per node**, each running `zim daemon run --port <port>` with `ZIM_HOME=data/<nick>`.
- **Window 1 `cli`** — one **pane per node**, each with `ZIM_HOME` pinned (so a bare `zim …` targets that peer) and the `alice`/`bob` functions sourced (so you can reach the others). A header line names the pane's peer.
- **Window `hub`** — added by `hub up` / `--hub`; runs `zim-hub`.

```bash
Ctrl+b 0/1          # switch windows
Ctrl+b ↑/↓          # switch panes
Ctrl+b [            # scroll mode (q to exit)
Ctrl+b d            # detach (session keeps running)
tmux attach -t zim-dev
tmux kill-session -t zim-dev
```

### Seeding fixtures (`./bin/dev seed`)

Daemon-only (no docker/hub). Cross-adds every peer to every other's address book, then the owner (`alice`) creates `demo` + `notes` vaults with content and shares `demo` with the other peers. Idempotent. Verify sync propagated:

```bash
./bin/dev cli bob vault list
./bin/dev cli bob vault cat demo /readme.md
```

P2P discovery takes a few seconds; the seed step waits for dialability before sharing, and re-running picks up anything that hadn't converged.

## Hub in dev (`./bin/dev hub`)

The hub embeds a peer (`zim::ServiceState`) and serves the web mirror. In dev it needs docker (minio, via `bin/minio`) for the S3 blob store.

```bash
./bin/dev hub up       # minio + web SPA build + zim-hub in the `hub` tmux window
./bin/dev hub enroll   # seed the hub user + enroll all dev daemons (see below)
./bin/dev hub did      # print the hub's did:web (off /.well-known/did.json)
./bin/dev hub down     # kill the hub window
```

**OAuth.** `hub up` resolves real Google OAuth via `confit` when it's installed, so the web view is usable; without `confit` it boots with dummy creds (web login disabled, daemon↔hub sync still works — that path is pure iroh). The admin email defaults to `$ZIM_DEV_SEED_EMAIL` (default `al@krondor.org`).

**Hub coupling is opt-in.** All hub commands live under `zim hub` (`zim hub login`, `zim hub peers sync`, `zim hub peers ls`). A daemon that only syncs with your own devices over p2p never needs any of them — the base `zim peers add|list|rm|ping` manage the address book directly with no hub involved. The hub is, at this point, mostly device management: it tracks your account's device roster (a `did:web` document at `/u/<user>/did.json`, one verification method per enrolled key), and `zim hub peers sync` folds that roster into the local `peers.toml` so all your devices know each other without manual cross-adds.

**Enrollment without the device-code dance.** A daemon normally pairs via `zim hub login` (browser device-code approval). For dev, `hub enroll` skips that: it calls the `zim-hub-devseed` binary to write the rows a successful login would have produced directly into the hub DB (`$ZIM_HUB_HOME/state/hub.db`):

- one admin+authorized `User` for `$ZIM_DEV_SEED_EMAIL`
- one `Daemon`-kind `UserPeer` row per dev node (keyed by its `zim id` pubkey)

then writes a `hub-session.json` into each node's home and runs `zim hub peers sync` on it, so the daemons' address books federate off the hub roster — the same path a real login would take. It does **not** mint a browser web key — that's a browser-resident keypair you still onboard manually in the web UI (the workspace gates on it). Run the seeder standalone too:

```bash
ZIM_HUB_HOME=data/zim-hub ZIM_DEV_SEED_EMAIL=al@krondor.org \
  ./target/debug/zim-hub-devseed alice=<pubkey-hex> bob=<pubkey-hex>
```

**One-shot.** `./bin/dev --hub` runs the whole sequence: start daemons → wait for them → `hub up` → wait for the hub → `seed` (p2p cross-add, so alice & bob know each other even without the hub) → `hub enroll` (hub rows + `hub peers sync`), then drops you in tmux. Finish by signing into the web view as the seed email and minting your web key.

### MinIO (S3-compatible blob storage)

```bash
./bin/minio up       # container `zim-minio`, API :17180, console :17181 (17xxx port convention)
./bin/minio down
./bin/minio status
```

Credentials `minioadmin:minioadmin`, bucket `zim-blobs`. Prefers `podman`, falls back to `docker`; force with `ZIM_CONTAINER_RUNTIME`.

## FUSE mounting

FUSE is opt-in in dev. `./bin/dev run --fuse` (or `export ZIM_DEV_FUSE=1`) builds the daemon with the `fuse` feature so `zim mount` works; without it the daemons build plain and `zim mount` 404s. The flag errors loudly if no native lib is found (install macFUSE on macOS, `libfuse3-dev` on Linux).

FUSE correctness is exercised by the fixture system: `./bin/dev seed` (or `./bin/dev fixtures apply`) runs the declarative `fuse_*` fixtures in `bin/dev_/fixtures.toml` — mount, read/write/mv/rm through the mountpoint, each write cross-checked against the vault via `vault_read` — and auto-skips them when FUSE is unavailable (`./bin/dev fuse-check` reports which). Fixture failures are real failures; skips are not.

## Inspecting node state

Each node's home (`data/<nick>/`) holds everything for that peer:

```
data/alice/
├── config.toml      # api_port (seeded by bin/dev)
├── identity.key     # Ed25519 secret, hex
├── log.sqlite       # SqliteVaultLog — every known vault head
├── peers.toml       # address book (TomlPeerStore)
├── blobs/           # content store
└── state/daemon.log
```

Inspect the vault log directly:

```bash
sqlite3 data/alice/log.sqlite '.tables'
sqlite3 data/alice/log.sqlite 'SELECT * FROM vault_log ORDER BY height DESC LIMIT 10;'
```

The hub's app state (users, devices, escrow) is separate, at `data/zim-hub/state/hub.db`.

## Cleanup

```bash
./bin/dev clean        # remove each node's data dir
./bin/dev kill         # kill the tmux session
./bin/dev kill --force # also kill orphaned processes on dev ports
make cleanup-all       # nuke cargo target + dev data + minio
```
