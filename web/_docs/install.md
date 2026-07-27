---
title: Install
order: 3
---

## Supported platforms

Prebuilt binaries:

| Platform | Standard | FUSE mounts |
|----------|----------|-------------|
| macOS (Apple Silicon) | ✓ | ✓ (requires [macFUSE](https://macfuse.github.io/)) |
| Linux (x86_64) | ✓ | ✓ (requires libfuse3) |

Anything else (Linux ARM, Intel Macs, WSL2): build from source below.

## Install

One line, latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/krondor-corp/zim/main/install.sh | sh
```

Variants:

```bash
# with FUSE mount support
curl -fsSL https://raw.githubusercontent.com/krondor-corp/zim/main/install.sh | sh -s -- --fuse

# a specific version
curl -fsSL https://raw.githubusercontent.com/krondor-corp/zim/main/install.sh | sh -s -- --version 0.1.0
```

The script installs to `~/.local/bin` (override with `ZIM_INSTALL_DIR`) and tells you if that's missing from your `PATH`.

### FUSE variant

The `-fuse` builds let you mount vaults as regular folders (`zim mount`). They need the platform FUSE library installed first:

- **macOS** — [macFUSE](https://macfuse.github.io/)
- **Linux (Debian/Ubuntu)** — `sudo apt install fuse3`
- **Linux (Fedora)** — `sudo dnf install fuse3`

Everything except `zim mount` works identically in the standard build.

### From source

Needs a Rust toolchain ([rustup.rs](https://rustup.rs)):

```bash
cargo install --locked --git https://github.com/krondor-corp/zim zim --features hub

# with FUSE support too (needs libfuse3 / macFUSE headers)
cargo install --locked --git https://github.com/krondor-corp/zim zim --features hub,fuse
```

The `hub` feature compiles the `zim hub …` commands (enrolling with a
hub, device management) — the prebuilt binaries include it.

### Verify

```bash
zim version
```

## Updates

The CLI updates itself from GitHub releases:

```bash
zim update --check   # report what's available
zim update           # download, swap the binary, restart the daemon service if installed
```

## Run the daemon as a service

The daemon manages its own OS service registration — no unit files to write:

```bash
zim daemon service install    # register with launchd (macOS) / systemd (Linux)
zim daemon service start
zim daemon service status
```

`stop` and `uninstall` undo it. Logs: `zim daemon logs`.

To run it in the foreground instead (debugging, containers):

```bash
zim daemon run
```

The daemon listens on `127.0.0.1:17171` — loopback only. Override the port in `config.toml` (`api_port`) or with `zim daemon run --port`.

## Troubleshooting

**`command not found: zim`** — add the install dir to your `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

**Daemon won't start / port in use** — something else holds `17171`. Change `api_port` in `~/.config/zim/config.toml`, or find the squatter: `lsof -i :17171`.

**FUSE mount fails** — make sure the platform library is present (macFUSE on macOS, `fuse3` on Linux) *and* you installed a `--fuse` build: `zim version` should say so, and `zim mount add` will tell you when support isn't compiled in.

**Fresh start** — your data lives in `~/.config/zim` (or `$ZIM_HOME`). Stop the daemon, move that directory aside, and `zim init` again. `identity.key` in that directory is your device identity — back it up before deleting anything.
