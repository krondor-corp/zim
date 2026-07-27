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
make install-dev

# OR release install
make install
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

| | Release (`make install`) | Debug (`make install-dev`) |
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
# 1. Initialize the local peer and start the daemon.
zim init
zim daemon run

# 2. In another terminal, create a vault.
zim vault create demo

# 3. Use it.
zim id
zim vault demo mkdir /docs
echo "hello zim" | zim vault demo add /docs/readme.md
zim vault demo ls /docs
zim vault demo cat /docs/readme.md
zim vault demo head
```

Data lives in `$ZIM_HOME` (release default `~/.config/zim`):

```
~/.config/zim/
├── config.toml
├── identity.key
├── log.sqlite
├── blob-index.sqlite
├── blobs/
└── state/
```

Override the location:

```bash
ZIM_HOME=/tmp/test zim init
ZIM_HOME=/tmp/test zim daemon run
```

The daemon listens on loopback. Configure its API port in
`$ZIM_HOME/config.toml` before starting it.

## Multi-peer local dev environment

To exercise sync between two peers without juggling terminals, use `./bin/dev`:

```bash
./bin/dev                    # spawn 2 daemons (alice, bob) in tmux and attach
./bin/dev run --background   # spawn without attaching
./bin/dev status             # which daemons are up
./bin/dev cli alice id       # run any `zim …` command against alice's daemon
./bin/dev cli bob vault create demo
./bin/dev kill --force       # tear down, free ports
./bin/dev clean              # delete the per-peer data dirs under ./data
```

Node config: `bin/dev_/nodes.toml`. Add more nodes by appending sections.

## Re-installing after code changes

```bash
# Dev install: just rebuild — symlink stays current.
cargo build -p zim

# Release install: re-run installer with --force.
make install
```

## Uninstall

```bash
make uninstall               # remove the binary, keep your data
make uninstall-purge         # also delete $ZIM_HOME
```

## Troubleshooting

**`error: vault not found`**

The daemon is up but no matching vault exists. Create one with
`zim vault create <name>`.

**`error: transport: ... Connection refused`**

The daemon isn't running on the endpoint you're pointing at. Start it with
`zim daemon run` or pass `--endpoint http://...` to the failing command.

**Two daemons on the same port**

Use `./bin/dev`; it assigns isolated homes and ports from
`bin/dev_/nodes.toml`.
