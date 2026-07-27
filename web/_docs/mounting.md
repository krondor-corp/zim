---
title: Mounting Buckets
order: 5
---

Mount a bucket as a local directory. Read and write files through your filesystem — `ls`, `cat`, `cp`, editors — while the daemon handles encryption and sync underneath.

## Prerequisites

FUSE must be installed:

**macOS (Apple Silicon):**

```bash
brew install --cask macfuse
```

**Linux (Ubuntu/Debian):**

```bash
sudo apt install libfuse3-dev
```

**Linux (Gentoo):**

```bash
emerge -av sys-fs/fuse:3
```

Your user must be in the `fuse` group on Linux:

```bash
sudo gpasswd -a $USER fuse
# Log out and back in for the group change to take effect.
```

The `zim` binary must be built with FUSE support (the default). If you installed via the install script, the FUSE variant is available on macOS Apple Silicon only.

## Mount a bucket

```bash
zim fs mount my-notes /mnt/my-notes
```

The directory is created if it doesn't exist. The daemon must be running.

## Unmount

```bash
zim fs unmount my-notes
```

Or use the system unmount command:

```bash
# macOS
umount /mnt/my-notes

# Linux
fusermount -u /mnt/my-notes
```

## List active mounts

```bash
zim fs list
```

## What you can do

Once mounted, standard filesystem operations work:

```bash
ls /mnt/my-notes/
cat /mnt/my-notes/readme.md
cp ~/photos/vacation.jpg /mnt/my-notes/
mkdir /mnt/my-notes/docs
echo "hello" > /mnt/my-notes/note.txt
```

Every write encrypts the data, stores it as a content-addressed blob, and advances the bucket's manifest chain. The daemon syncs changes to connected peers automatically.

## Limitations

- **No partial writes.** Editing a large file re-encrypts and re-uploads the whole file on flush.
- **Cache TTL.** File metadata and content are cached locally (default 60 seconds). Changes from remote peers may take up to one minute to appear in the mount.
- **Single writer.** FUSE mount assumes single-writer semantics. Concurrent writes from another device sync normally but may produce conflict files (see the sync documentation for how conflicts are resolved).

## Troubleshooting

### "Transport endpoint is not connected"

The daemon was stopped while a mount was active. Unmount and remount:

```bash
fusermount -uz /mnt/my-notes   # Linux (lazy unmount)
diskutil unmount force /mnt/my-notes  # macOS
zim fs mount my-notes /mnt/my-notes   # remount
```

### "Operation not permitted" on macOS

macFUSE requires a kernel extension. After installing macFUSE, go to System Settings > Privacy & Security and approve the extension. Reboot.

### Mount doesn't appear in Finder/Nautilus

FUSE mounts are visible via the terminal. Some file managers require additional configuration to show FUSE mounts. Use `ls` or `open /mnt/my-notes` (macOS) as a workaround.
