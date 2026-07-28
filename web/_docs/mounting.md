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
zim mount remove notes        # unmount and forget it
```

Auto-mounts come back on daemon start; a mount stopped with `stop` can
be started again, while `remove` forgets it entirely.

## How it works

Every write through the mount encrypts the data, stores it as a
content-addressed blob, and advances the vault's version chain — exactly
as `zim vault add` does. Reads decrypt on demand. Changes sync to
shareholders (and to the browser) automatically.
