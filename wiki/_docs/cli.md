---
title: CLI Reference
order: 4
---

The `zim` command-line tool manages buckets, runs the daemon, and controls P2P sync.

## Setup

```bash
zim init                    # one-time: create state dir + identity
zim daemon                  # start the daemon (foreground)
```

The daemon must be running for most commands to work. Run it in one terminal, use the CLI in another.

## Buckets

### Create a bucket

```bash
zim bucket create my-notes
```

### List all buckets

```bash
zim bucket ls
```

### List files in a bucket

```bash
zim bucket ls my-notes /
zim bucket ls my-notes /docs/
```

### Add a file

```bash
zim bucket add my-notes ./readme.md
zim bucket add my-notes ./photos/  # add a directory
```

### Read a file

```bash
zim bucket cat my-notes /readme.md
```

### Create a directory

```bash
zim bucket mkdir my-notes /docs
```

### Move or rename

```bash
zim bucket mv my-notes /old-name.txt /new-name.txt
```

### Delete a file or directory

```bash
zim bucket rm my-notes /draft.md
```

### View version history

```bash
zim bucket history my-notes
```

### Bucket status

```bash
zim bucket stat my-notes
```

### Clone a bucket from a peer

```bash
zim bucket clone <share-link>
```

### Approve or ignore a pending bucket

```bash
zim bucket approve <bucket-id>
zim bucket ignore <bucket-id>
```

## Publishing

### Publish individual files or folders

```bash
zim bucket files publish my-notes /readme.md
zim bucket folders publish my-notes /docs/
```

### List published entries

```bash
zim bucket files list my-notes
zim bucket folders list my-notes
```

## Sharing & access

### List shares

```bash
zim bucket shares ls my-notes
```

### Create a share link

```bash
zim bucket shares create my-notes
```

## Mirrors

### Add a mirror (e.g. a zim-hub)

```bash
zim bucket mirror add my-notes <hub-node-id>
```

### Remove a mirror

```bash
zim bucket mirror remove my-notes <hub-node-id>
```

### List mirrors

```bash
zim bucket mirror list my-notes
```

## Viewer management

### List pending viewer requests

```bash
zim bucket viewer list my-notes
```

### Authorize a viewer

```bash
zim bucket viewer authorize my-notes <viewer-pubkey>
```

### Deauthorize a viewer

```bash
zim bucket viewer deauthorise my-notes <viewer-pubkey>
```

## Sync

### Manually trigger sync with a peer

```bash
zim bucket sync now my-notes <node-id>
```

### List sync peers

```bash
zim bucket sync list my-notes
```

## FUSE mounting

Mount a bucket as a local directory (requires FUSE support — see [Mounting Buckets]({{ '/docs/mounting/' | relative_url }})):

```bash
zim fs mount my-notes /mnt/my-notes
zim fs unmount my-notes
zim fs list
```

## Daemon

```bash
zim daemon                  # start (foreground)
zim daemon --gateway-only   # gateway mode (no full API)
```

## Hub

```bash
zim hub register --hub https://hub.example.com   # register this device with a hub
zim hub login --hub https://hub.example.com       # refresh auth token
```

## Other

```bash
zim health          # check daemon health
zim version         # print version
zim update          # self-update to latest release
zim --plain ...     # no colors, no table borders (for scripting)
```

## Global options

| Flag | What it does |
|------|-------------|
| `--remote <URL>` | Target a specific daemon URL instead of the default. |
| `--config-path <PATH>` | Use a custom state directory (defaults to `~/.config/zim/`). |
| `--plain` | Plain text output — no colors, no table borders. |
