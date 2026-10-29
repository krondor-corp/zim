---
title: Getting started
order: 1
---

Zim keeps your files and notes in **vaults** — encrypted, versioned
folders that sync between your browser and your devices. Everything is
encrypted before it leaves your hands; the hub that stores and relays
your data only ever sees ciphertext.

The fastest way in is the browser. No installs, three steps.

## 1. Sign in

Open **[hub.zim.krondor.org](https://hub.zim.krondor.org)** and sign in
with Google.

The first time in, you'll be asked to set a **passphrase**. This creates
your web key — the identity your browser uses to encrypt and decrypt.
The passphrase wraps that key before it's stored, so the hub can hold it
for you without being able to use it.

> **Your passphrase is not recoverable.** It protects the key that
> decrypts your vaults; nobody — including the hub — can reset it for
> you. Put it in your password manager.

## 2. Create your first vault

In the browser, create a vault and give it a name. A vault is the
unit of everything in Zim: it has its own encryption secret, its own
version history, and its own list of devices that can open it.

You start as the only member. Every file you add is encrypted under the
vault's secret; the vault's history advances as a signed chain of
versions, so any device can verify it's seeing the real thing.

## 3. Work in the browser

Inside a vault you can:

- **Browse** the file tree — expand folders, click a file to view it.
- **Upload** files and create folders.
- **Write notes** — create a markdown file and edit it in the browser, with formatting shown as you type. Changes save automatically.
  Changes save automatically a moment after you stop typing (or right
  away with Cmd/Ctrl+S); each save commits a new version to the
  vault's history.
- **Inspect** the vault's details — its id, current version, and the
  devices it's shared with.
- **Go back in time** — **History** lists every version, newest first.
  Pick one to browse the vault exactly as it was, open any file, and
  **Restore this file** to bring that content back as a new version.
  Files that haven't changed since that version show as unchanged —
  there's nothing to restore. Nothing is ever rewritten, so an
  accidental overwrite is undoable.
  Versions from before this browser was given access stay sealed.

Everything you do here is committed as an encrypted version and synced
to any other device that holds the vault.

## Next

The browser is one device. The other half of Zim is having the same
vaults on your machines — synced by the CLI, mounted as real folders:

- **[Your devices]({{ '/docs/devices/' | relative_url }})** — connect
  the CLI, sync your device roster, and mount vaults locally.
- **[Install]({{ '/docs/install/' | relative_url }})** — all the ways
  to get the `zim` binary.
