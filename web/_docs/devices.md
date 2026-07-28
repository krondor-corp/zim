---
title: Your devices
order: 2
---

The browser is one device. This page connects the rest: sign the
CLI into your account, sync your device roster, and (optionally) mount
vaults as local folders.

## 1. Install and sign in on the CLI

Install the `zim` binary ([all the options]({{ '/docs/install/' | relative_url }})):

```bash
curl -fsSL https://raw.githubusercontent.com/krondor-corp/zim/main/install.sh | sh
```

Then pair this machine with your account:

```bash
zim hub login --hub https://hub.zim.krondor.org
```

The terminal prints a URL and a short code. Open the URL in a browser
where you're already signed in, enter the code, and approve the device.
That approval enrolls this machine's key into your account — the CLI
never sees your Google credentials.

## 2. Sync your device roster

```bash
zim hub peers sync
```

This pulls your account's device roster from the hub into this
machine's address book — the web key, your other machines, and the hub
itself.

> **This step is what connects web and local.** Until a device has
> synced the roster, it doesn't know your other devices exist — vaults
> created in the browser **will not appear** on the CLI, and vaults
> shared from the CLI won't reach the web, no matter how long you wait.
> If the two sides ever look out of sync, run `zim hub peers sync`
> again first.

Check what the account knows:

```bash
zim hub peers ls
```

Devices marked as in your address book are the ones this machine will
sync with. From here, vaults flow both ways automatically — browser
edits land on your machine within moments, and local writes appear in
the browser.

## 3. Mount a vault (optional, needs FUSE)

If FUSE is installed ([macFUSE](https://macfuse.github.io) on macOS,
`fuse3` on Linux) and you installed the FUSE build of zim, a vault can
be mounted as a real folder:

```bash
zim mount add <vault> ~/zim/notes
```

Anything you drop in the folder is encrypted and versioned like any
other vault write — and syncs everywhere, including the browser.
See [Mounting]({{ '/docs/mounting/' | relative_url }}) for the full
lifecycle (auto-mount, read-only, unmounting).

## Day-to-day

```bash
zim vault list                  # vaults this machine holds
zim vault cat <vault> /note.md  # read a file
zim vault add <vault> /note.md  # write a file (stdin)
zim update                      # update the binary + daemon
```

The full command surface is in the [CLI reference]({{ '/docs/cli/' | relative_url }}).
