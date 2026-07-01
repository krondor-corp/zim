# Installing Zim locally

Two install modes:

- **Release** — `cargo install`-style. One shot, slow build, fast runtime.
- **Dev (recommended for hacking)** — symlinks `~/.cargo/bin/zim` to the debug build. Edit code, `cargo build -p zim`, run `zim ...` immediately. No reinstall step.

Both put `zim` in `~/.cargo/bin/`, so make sure that's on your `$PATH`.

## Prerequisites

- Rust toolchain (`rustup`), edition 2021 — stable is fine.
- `tmux` if you want to use `./bin/dev` to spin up multiple peers locally.
- macOS / Linux. Windows untested.

## Install

```bash
# clone if you haven't already
git clone https://github.com/zim/zim
cd zim

# dev install — fast iteration loop
./bin/install --dev

# OR release install
./bin/install
```

Confirm it's on PATH:

```bash
zim --help
```

If you see `command not found`, add cargo's bin dir to your PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## Debug vs release profile

The dev install (`--dev`, a debug build) and the release install behave differently by design — keyed on `debug_assertions`, so it's automatic, no flags. This keeps a dev binary from clobbering a real install's state:

| | Release (`./bin/install`) | Debug (`./bin/install --dev`) |
|---|---|---|
| Default data dir | `~/.config/zim` | `~/.config/zim/debug` |
| Default log level | `info` | `debug` |
| `zim clean` command | absent | present |

Only the *default* location is nested under `debug/` — an explicit `--config-path` or `$ZIM_HOME` is always used verbatim, so `./bin/dev`'s per-peer homes are unaffected.

`zim clean` wipes the resolved data dir (the `debug/` one on a dev binary). It's a dry run by default — it lists what would be removed — and only deletes with `--yes`:

```bash
zim clean          # dry run: shows ~/.config/zim/debug and its contents
zim clean --yes    # actually delete it
```

It exists only in debug builds, so a release binary can never use it to nuke real data.

## First run

The daemon owns the runtime; the CLI is a thin client that POSTs to it.

```bash
# 1. Start the daemon (blocks; or run in a separate terminal / tmux pane).
zim daemon

# 2. In another terminal, bootstrap a vault.
zim init demo

# 3. Use it.
zim id                          # show this peer's pubkey
zim mkdir /docs
echo "hello zim" | zim add /docs/readme.md
zim ls /docs
zim cat /docs/readme.md
zim head
```

Data lives in `$ZIM_HOME` (default `~/.zim`):

```
~/.zim/
├── identity.key       # Ed25519 secret, hex-encoded
├── vault.uuid         # UUID of the (one) vault
├── log.sqlite         # append-only version log
└── blobs/             # iroh-blobs filesystem store
```

Override the location:

```bash
ZIM_HOME=/tmp/test zim daemon
ZIM_HOME=/tmp/test zim init demo
```

The daemon listens on `127.0.0.1:17171` by default (loopback-only). Override:

```bash
zim daemon --port 17180
zim --endpoint http://127.0.0.1:17180 id
```

## Multi-peer local dev environment

To exercise sync between two peers without juggling terminals, use `./bin/dev`:

```bash
./bin/dev                    # spawn 2 daemons (alice, bob) in tmux and attach
./bin/dev run --background   # spawn without attaching
./bin/dev status             # which daemons are up
./bin/dev cli alice id       # run any `zim …` command against alice's daemon
./bin/dev cli bob init demo  # bootstrap bob
./bin/dev kill --force       # tear down, free ports
./bin/dev clean              # delete the per-peer data dirs under ./data
```

Node config: `bin/dev_/nodes.toml`. Add more nodes by appending sections.

## Re-installing after code changes

```bash
# Dev install: just rebuild — symlink stays current.
cargo build -p zim

# Release install: re-run installer with --force.
./bin/install
```

## Uninstall

```bash
./bin/uninstall              # remove the binary, keep your data
./bin/uninstall --purge      # also delete $ZIM_HOME (default ~/.zim)
```

## Troubleshooting

**`error: api: no vault initialized — run \`zim init <name>\``**

The daemon is up but no vault has been created in this `$ZIM_HOME`. Run `zim init <name>` first.

**`error: transport: ... Connection refused`**

The daemon isn't running on the endpoint you're pointing at. Start it (`zim daemon`) or pass `--endpoint http://...` to the failing command.

**Two daemons on the same port**

The daemon binds loopback by default. Use `--port` to give each their own:

```bash
ZIM_HOME=~/.zim-alice zim daemon --port 17171
ZIM_HOME=~/.zim-bob   zim daemon --port 17172
zim --endpoint http://127.0.0.1:17172 init bob-demo
```

This is what `./bin/dev` automates.
