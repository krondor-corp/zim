---
title: Mounting
order: 5
---

Mount a vault as a local folder. Read and write files through your
filesystem — `ls`, `cat`, `cp`, editors — while the daemon handles
encryption and sync underneath. Anything you write is encrypted,
versioned, and synced like any other vault change.

## Prerequisites

FUSE must be installed:

- **macOS** — [macFUSE](https://macfuse.github.io)
- **Linux** — `fuse3` (`libfuse3` on most distros)

The `zim` binary must be built with FUSE support. The install script
ships the FUSE build on macOS (Apple Silicon) and Linux (x64); on other
platforms, build from source with `--features fuse`.

## Mount a vault

```bash
zim mount add notes ~/zim/notes
```

The mountpoint is created if it doesn't exist. The daemon must be
running. The vault now appears as a normal folder — edit it with any
tool.

## Manage mounts

```bash
zim mount list                # mounts and their status
zim mount set notes --auto    # remount automatically when the daemon starts
zim mount set notes --ro      # remount read-only
zim mount stop notes          # unmount, keep the registration
zim mount start notes         # mount a stopped registration again
zim mount remove notes        # unmount and forget it
```

Only `--auto` mounts come back on their own when the daemon starts. Any
other registration — or one you unmounted with `stop` — stays unmounted
until you run `zim mount start`, which mounts it again with its saved
`--auto` / read-only settings. `remove` forgets a mount entirely.

A path holds one mount. `zim mount add` refuses a path that's already
registered — use `start` to re-mount it, `set` to change its flags, or
`remove` then `add` to put a different vault there.

`stop` is safe to repeat. If the daemon was killed without a clean
shutdown and left a dead volume behind (it shows in Finder or `mount` but
hangs on access), `zim mount stop notes` clears it.

## How it works

Every write through the mount encrypts the data, stores it as a
content-addressed blob, and advances the vault's version chain — exactly
as `zim vault add` does. Reads decrypt on demand. Changes sync to
shareholders (and to the browser) automatically.
