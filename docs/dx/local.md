# Development

User-facing local development guide (build, run daemon, run hub, hot reload) lives in the site docs: [Local Development](../../web/_docs/local-development.md).

This file covers contributor-specific multi-peer dev tooling that doesn't belong in end-user docs.

## Multi-peer dev environment (`bin/dev`)

`bin/dev` spins up a multi-node P2P network in tmux so you can exercise sync, sharing, mounting, and the hub locally. It builds the workspace's debug binary once (`cargo build -p zim` → `target/debug/zim`) and runs that same binary for every peer — **not** `cargo run`, and **not** an installed `~/.cargo/bin/zim`. Peers are isolated purely by `ZIM_HOME`.

Nodes are defined in `confit.toml`'s `[dev.nodes]`. The key is the peer's nick (`alice`, `bob`), its data dir (`data/<nick>/`), its tmux pane label, and the name of the generated shell function; the value is its daemon port, resolved per worktree (next section).

```toml
[ports.services]
alice = 10
bob   = 20

[dev.nodes]
alice = "{ports.services.alice}"
bob   = "{ports.services.bob}"
```

### Ports and several worktrees at once

`bin/dev` needs [confit](https://confit.krondor.org) (a required tool, like cargo; `bin/dev` exits with the install one-liner if it's missing). At startup it resolves `[env.dev]` from `confit.toml`, which gives this worktree its own ports and names, so several jig worktrees (plus `main`) can each run `./bin/dev --hub` at the same time:

| Resource | Source | `main` (slot 0) | worktree in slot N |
| -- | -- | -- | -- |
| daemon ports | `[dev.nodes]` → `ports.services.<nick>` lane | alice 17110, bob 17120 | 17110+N, 17120+N |
| hub port | `ports.services.hub` | 17190 | 17190+N |
| tmux session | `ports.slug` | `zim-dev` | `zim-dev-<slug>` |
| minio bucket | `ports.slug` | `zim-blobs` | `zim-blobs-<slug>` (capped at 63 chars) |
| minio ports | `ports.infra.*` | 17180 / 17181 | same: one container for everyone |

confit hands out slots per clone: `main` always gets 0, any other branch the lowest free 1–9. The ledger lives at `<git-common-dir>/confit/ports.toml`, so every worktree sees it, and entries for removed worktrees drop off on the next resolve. That caps you at 9 non-`main` worktrees running at once. `./bin/dev ports` prints this worktree's slot, ports, tmux session, hub URL, and bucket; `confit validate` checks the layout (lanes 10 apart, no duplicate ports, nothing already bound on this host).

What each worktree keeps to itself:

- **Daemons and hub** bind their own ports. `./bin/dev kill --force` only frees *this* worktree's listening ports, so it never touches another worktree's processes.
- **Data** is already per worktree (`data/` lives in the checkout), so identities and hub DBs never mix.
- **Buckets.** `hub up` creates the worktree's bucket (`./bin/minio bucket <name>`) and points the hub at it. Each hub keeps its own blob index and may delete objects, so hubs never share a bucket.
- **Browser sessions.** A loopback hub names its session cookie `session-<port>` (production keeps `session`). Browsers scope cookies by host and ignore the port, so with one shared name, signing into one worktree's hub would log you out of another's.

To add a node: add a `ports.services.<nick>` lane (10 from the others) and a `<nick> = "{ports.services.<nick>}"` line under `[dev.nodes]`, both in `confit.toml`. `bin/dev` reads the section once with `confit show dev.nodes` (`nick=port` lines); `zim-e2e` reads only its keys, as plain TOML. iroh endpoints already bind random ports and need no setup. `make e2e` doesn't use confit: it picks free ports at boot.

### Commands

```bash
./bin/dev                       # = run; build + start daemons in tmux (attaches)
./bin/dev run -b|--background   # start detached
./bin/dev run --fuse            # build daemons with the `fuse` feature (zim mount)
./bin/dev run --hub             # full stack: daemons + hub + fixtures + enroll
./bin/dev --hub                 # shorthand for `run --hub`
./bin/dev status                # which daemons answer /_status/livez
./bin/dev ports                 # this worktree's slot, ports, tmux session, bucket
./bin/dev cli <nick> <args...>  # run `zim` against one node (sets its ZIM_HOME)
./bin/dev shell                 # emit `alice`/`bob` shell functions (see below)
./bin/dev logs <nick>           # dump that node's tmux pane
./bin/dev seed                  # create vaults + cross-shares (daemon-only)
./bin/dev hub [up|enroll|did|down]
./bin/dev clean                 # rm -rf each node's data dir
./bin/dev kill [-f|--force]     # kill the tmux session; -f also frees this worktree's ports
```

`make dev` is `./bin/dev --hub` with FUSE on: the whole stack. For anything finer (daemons only, status, a node's CLI), call `./bin/dev` directly.

`make e2e` runs the `zim-e2e` crate: a hermetic one-shot verdict — it spawns its own daemons on free ports picked at boot (bind `127.0.0.1:0`, read the port, release it), with fresh homes under `data/e2e/`, so it never touches the interactive dev env and runs in several worktrees at once, wires peers with direct NodeAddrs (`/api/v0/peers/{addr,introduce}`, no DHT), applies `bin/dev_/fixtures.toml` through the real CLI, and polls cross-node convergence to a deadline (`--deadline`, default 60s). `--keep` leaves the throwaway env running; `--skip-fuse` forces the FUSE block off. Exit code is the verdict. The harness is a crate (not a script) so the hub repo can reuse it against released `zim-cli` binaries after the split.

### Per-peer shells

`./bin/dev shell` prints one function per node that wraps the binary with the right `ZIM_HOME`:

```bash
eval "$(./bin/dev shell)"
alice vault list
bob vault cat demo /readme.md
```

### Tmux layout

The session (`zim-dev` on `main`, `zim-dev-<slug>` in other worktrees; see `./bin/dev ports`) has:

- **Window 0 `nodes`** — one **pane per node**, each running `zim daemon run --port <port>` with `ZIM_HOME=data/<nick>`.
- **Window 1 `cli`** — one **pane per node**, each with `ZIM_HOME` pinned (so a bare `zim …` targets that peer) and the `alice`/`bob` functions sourced (so you can reach the others). A header line names the pane's peer.
- **Window `hub`** — added by `hub up` / `--hub`; runs `zim-hub`.

```bash
Ctrl+b 0/1          # switch windows
Ctrl+b ↑/↓          # switch panes
Ctrl+b [            # scroll mode (q to exit)
Ctrl+b d            # detach (session keeps running)
./bin/dev attach    # = tmux attach -t <this worktree's session>
./bin/dev kill      # = tmux kill-session -t <this worktree's session>
```

### Seeding fixtures (`./bin/dev seed`)

Daemon-only (no docker/hub). Cross-adds every peer to every other's address book, then the owner (`alice`) creates `demo` + `notes` vaults with content and shares `demo` with the other peers. Idempotent. Verify sync propagated:

```bash
./bin/dev cli bob vault list
./bin/dev cli bob vault cat demo /readme.md
```

P2P discovery takes a few seconds; the seed step waits for dialability before sharing, and re-running picks up anything that hadn't converged.

## Hub in dev (`./bin/dev hub`)

The hub embeds a peer (`zim::ServiceState`) and serves the web mirror. In dev it needs a container runtime, docker or podman, for the S3 blob store (via `bin/minio`).

```bash
./bin/dev hub up       # minio + web SPA build + zim-hub in the `hub` tmux window
./bin/dev hub enroll   # seed the hub user + enroll all dev daemons (see below)
./bin/dev hub did      # print the hub's did:web (off /.well-known/did.json)
./bin/dev hub down     # kill the hub window
```

**The web SPA** (`crates/zim-hub/web`) is its own cargo workspace, built for `wasm32` by Trunk (`make build-web`), so the root `cargo` commands don't cover it. Lint it with `cargo clippy --target wasm32-unknown-unknown -- -D warnings` from that directory. Its platform-free logic (e.g. the autosave `SaveQueue`) lives in the crate's `lib.rs` so `cargo test --lib` runs it natively; the binary itself only compiles for `wasm32`. The vault page reaches storage only through `backend::VaultBackend` (today a `WasmFs` on the hub). The SPA links the SDK as a Rust crate, so the SDK hands it domain types (`VaultId`, `VaultInfo`, `DirEntry`, `VersionSummary`, keys and DIDs), never JSON; only the hub's HTTP routes cross a wire. The page owns saving: editors emit text changes, and the page batches them into one write at most every 30s (the first unsaved edit arms the timer), one write at a time, flushing early on file switch, History, vault switch, tab hide, tab close (with a leave-site prompt while a draft is still unwritten), and Cmd/Ctrl+S. Cmd/Ctrl+K (or the header's search button) opens the command palette (`pages/vault_palette.rs`): one list of every file in the vault (walked via `backend.ls`) and the page's actions, filtered by `fuzzy.rs`'s subsequence scorer; choosing runs the same callbacks as the tree and header.

**The `.md` editor.** `.md` notes open in Milkdown (Crepe, WYSIWYG, `pages/markdown_editor.rs`); every other text file stays a textarea. Both honor the same contract (`EditorProps`: the file + `on_change`). The Crepe bundle is a vendored static asset (`crates/zim-hub/static/milkdown/`: `milkdown.js`, `milkdown.css`, KaTeX woff2 fonts), imported dynamically on the first `.md` open so the SPA's initial load doesn't carry it; it runs under the existing CSP (no `unsafe-eval`). Rebuild it with `crates/zim-hub/web/vendor/milkdown/build.sh` (node + npm; Crepe and esbuild are pinned in its `package.json`) and commit the output. Milkdown re-serializes markdown on load (list markers, escaping), so `getMarkdown()` right after mount is the baseline and `on_change` fires only for text that differs from it (`markdown::is_edit`); opening a note never autosaves. If you hand-write markdown in a non-canonical style, the first real edit will normalize the whole file to Milkdown's style.

**Login.** The hub's login providers are a list in `ZIM_HUB_AUTH` (unset means `google`). Sign-in is separate from the wasm app: the hub only serves the app to signed-in, approved users and sends everyone else to the server-rendered `/auth/login` (or `/auth/pending`), a chooser with one button per enabled provider (even if there's only one) that returns you to the page you asked for (`?next=`, carried through Google's round trip in the OAuth `state` param). `hub up` builds with `--features dev-login` and defaults the list to `dev-trust`: open the hub URL from `./bin/dev ports` (`http://127.0.0.1:17190` on `main`), type any email, and you're signed in as an admin. No Google account, OAuth client, or credential store needed. Type `$ZIM_DEV_SEED_EMAIL` (default `dev@example.com`) to land on the account `hub enroll` seeds. Accounts are bound to the provider that created them and never linked, so the seeded (dev-trust) account can't be signed into through Google, and an email first used with Google can't be signed into through dev-trust. Browser page loads on any other hostname (e.g. `localhost:17190`) are redirected to `HOST_NAME` (`127.0.0.1:17190`): the hub's `did:web` identity is fixed at startup and the browser builds DIDs from the address bar, so the two must agree. Dev-trust is refused unless `HOST_NAME` is loopback, and builds without the feature (release/Docker) refuse to start if it's listed. To exercise real Google OAuth too, list both; the sign-in page then offers both:

```bash
ZIM_HUB_AUTH=dev-trust,google GOOGLE_O_AUTH_CLIENT_ID=… GOOGLE_O_AUTH_CLIENT_SECRET=… ./bin/dev hub up
```

Every provider mints sessions through the same code (`http/auth/session.rs`); only who asserts the email differs.

The server-rendered `/auth/*` pages (sign-in chooser, dev login, pending, device approval) are askama templates in `crates/zim-hub/templates/auth/`. Each extends `base.html`, which mirrors the SPA's onboarding shell so both share `style.css`. Add new hub pages there; askama autoescapes, so don't hand-build HTML with `format!`.

**Hub coupling is opt-in.** All hub commands live under `zim hub` (`zim hub login`, `zim hub peers sync`, `zim hub peers ls`). A daemon that only syncs with your own devices over p2p never needs any of them — the base `zim peers add|list|rm|ping` manage the address book directly with no hub involved. The hub is, at this point, mostly device management: it tracks your account's device roster (a `did:web` document at `/u/<user>/did.json`, one verification method per enrolled key), and `zim hub peers sync` folds that roster into the local `peers.toml` so all your devices know each other without manual cross-adds.

**Device-code approval.** `zim hub login` prints the hub's `verification_url`, `/auth/device?code=…`: a server-rendered page (`http/auth/device.rs`), not part of the wasm app, so approving a daemon never asks for a web key or passphrase. Signed-out visitors go through `/auth/login?next=…` and come back. Approve POSTs `/auth/device/approve` (a same-origin form; the `SameSite=Lax` session cookie keeps other sites from submitting it), and the daemon's next signed poll enrolls it. The daemon side is a library flow (`zim::hub::login`: `start` → `PendingLogin::complete`, wire types in `zim_api::hub::auth::device_code`, shared with the hub handler) that never prints, so a desktop app sharing `$ZIM_HOME` can drive the same login with its own UI; the CLI op only renders the pairing instructions between the two calls. `/device?code=…`, the URL older hubs minted, redirects to `/auth/device`. To try it by hand against the dev hub: `./bin/dev cli alice hub login --hub <hub url>` and open the printed link.

**Enrollment without the device-code dance.** A daemon normally pairs via `zim hub login` (browser device-code approval). For dev, `hub enroll` skips that: it calls the `zim-hub-devseed` binary to write the rows a successful login would have produced directly into the hub DB (`$ZIM_HUB_HOME/state/hub.db`):

- one admin+authorized `User` for `$ZIM_DEV_SEED_EMAIL`
- one `Daemon`-kind `UserPeer` row per dev node (keyed by its `zim id` pubkey)

then writes a `hub-session.json` into each node's home and runs `zim hub peers sync` on it, so the daemons' address books federate off the hub roster — the same path a real login would take. It does **not** mint a browser web key — that's a browser-resident keypair you still onboard manually in the web UI (the workspace gates on it). Run the seeder standalone too:

```bash
ZIM_HUB_HOME=data/zim-hub ZIM_DEV_SEED_EMAIL=dev@example.com \
  ./target/debug/zim-hub-devseed alice=<pubkey-hex> bob=<pubkey-hex>
```

**One-shot.** `./bin/dev --hub` runs the whole sequence: start daemons → wait for them → `hub up` → wait for the hub → `seed` (p2p cross-add, so alice & bob know each other even without the hub) → `hub enroll` (hub rows + `hub peers sync`), then drops you in tmux. Finish by signing into the web view as the seed email and minting your web key.

### S3 blob store (RustFS, MinIO-compatible)

```bash
./bin/minio up             # container `zim-minio`, API :17180, console :17181 (confit `ports.infra`)
./bin/minio bucket <name>  # create a bucket (`hub up` makes this worktree's)
./bin/minio ls [bucket]    # objects a hub has mirrored (default `zim-blobs`, main's bucket)
./bin/minio down
./bin/minio status
```

Runs `docker.io/rustfs/rustfs` (MinIO pulled its public images; RustFS speaks the same S3 API on the same ports). Credentials `minioadmin:minioadmin`. One container serves every worktree; each worktree's hub gets its own bucket (`zim-blobs` on `main`, `zim-blobs-<slug>` elsewhere). Works with podman or docker: prefers whichever is running, podman first; force with `ZIM_CONTAINER_RUNTIME`. Bucket setup is a SigV4 `curl` from the host, so nothing is exec'd inside the container.

## FUSE mounting

FUSE is always on under `make dev`; with `./bin/dev` directly it's opt-in. `./bin/dev run --fuse` (or `export ZIM_DEV_FUSE=1`) builds the daemon with the `fuse` feature so `zim mount` works; without it the daemons build plain and `zim mount` 404s. The flag errors loudly if no native lib is found (install macFUSE on macOS, `libfuse3-dev` on Linux).

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
├── daemon.lock      # held while the daemon runs; {pid, api_port, started_at}
└── state/daemon.log
```

Inspect the vault log directly:

```bash
sqlite3 data/alice/log.sqlite '.tables'
sqlite3 data/alice/log.sqlite 'SELECT * FROM vault_log ORDER BY height DESC LIMIT 10;'
```

The hub's app state (users, devices, escrow) is separate, at `data/zim-hub/state/hub.db`.

## Checking the SPA on a phone

The hub SPA has one narrow-screen breakpoint: `@media (max-width: 700px)` at the end of `crates/zim-hub/static/style.css`. Everything above it is the desktop layout and must not change. Below it the vault page is list → detail: the tree fills the screen until a file opens, then the editor does, with a back arrow in the header (state in `VaultTree`, class `editor-layout--detail`; the pure rule is `nav::shows_detail`). Safe-area insets (`viewport-fit=cover` in `index.html`) keep the header and bottom docks clear of the notch and home indicator.

To check it without a device, run `make hub`, open the hub URL in Chrome DevTools device mode (iPhone, 390×844, touch), and walk: gate → workspace → vault tree → open a `.md` note → type → back → History. Or drive headless Chrome with the same emulation (390×844, DPR 3, `isMobile`, iPhone user agent). Compare against a 1440px-wide screenshot to confirm desktop is unchanged.

Icons are inline SVG (`components/icon.rs`, `layouts::menu_icon`), never text glyphs: iOS system fonts lack ☰ ⓘ ⧉ and render a missing-glyph box.

## Cleanup

```bash
./bin/dev clean        # remove each node's data dir
./bin/dev kill         # kill the tmux session
./bin/dev kill --force # also kill orphaned processes on dev ports
make dev-kill          # kill the session + free its ports
make dev-clean         # dev-kill + wipe dev data (./data) + minio
make rust-clean        # cargo clean
make clean             # everything: dev-clean + rust-clean + site build artifacts
```
