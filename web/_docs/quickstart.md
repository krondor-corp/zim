---
title: Getting started
order: 1
---

Zim keeps your files and notes in **vaults** — encrypted, versioned
folders that sync between your browser and your devices. Everything is
encrypted before it leaves your hands; the hub that stores and relays
your data only ever sees ciphertext.

The fastest way in is the web workspace. No installs, three steps.

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

From the workspace, create a vault and give it a name. A vault is the
unit of everything in Zim: it has its own encryption secret, its own
version history, and its own list of devices that can open it.

You start as the only member. Every file you add is encrypted under the
vault's secret; the vault's history advances as a signed chain of
versions, so any device can verify it's seeing the real thing.

## 3. Use the workspace

Inside a vault you can:

- **Browse** the file tree — expand folders, click a file to view it.
- **Upload** files and create folders.
- **Write notes** — create a markdown file, edit it in the workspace,
  and save. Each save commits a new version to the vault's history.
- **Inspect** the vault's details — its id, current version, and the
  devices it's shared with.

Everything you do here is committed as an encrypted version and synced
to any other device that holds the vault.

## Next

The workspace is one device. The other half of Zim is having the same
vaults on your machines — synced by the CLI, mounted as real folders:

- **[Your devices]({{ '/docs/devices/' | relative_url }})** — connect
  the CLI, sync your device roster, and mount vaults locally.
- **[Install]({{ '/docs/install/' | relative_url }})** — all the ways
  to get the `zim` binary.
