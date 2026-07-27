---
title: Quickstart
order: 2
---

From a fresh shell to an encrypted, syncable vault in four steps.

## 1. Install the CLI

```bash
curl -fsSL https://raw.githubusercontent.com/krondor-corp/zim/main/install.sh | sh
```

See [Install]({{ '/docs/install/' | relative_url }}) for the FUSE variant, source builds, and updates.

## 2. Initialize this device

```bash
zim init
```

This creates your data directory (`~/.config/zim` by default) with:

- `identity.key` — your Ed25519 device identity (back it up; never share it)
- `config.toml` — daemon configuration
- `blobs/` — the encrypted content store
- `state/` — the vault log and daemon state

The command prints your device's public key — that's how other devices and peers will know this one.

## 3. Start the daemon

```bash
zim daemon service install   # register with launchd / systemd
zim daemon service start
```

(Or run it in the foreground with `zim daemon run`.) The daemon listens on `127.0.0.1:17171` — loopback only, nothing is exposed off-host. Every other CLI command talks to it over this API.

## 4. Create and use a vault

```bash
zim vault create notes
echo "first note" | zim vault add notes /hello.md
zim vault ls notes /
zim vault cat notes /hello.md
```

`create` writes the vault's genesis manifest, with the vault secret sealed to your device key. `add` encrypts the content and advances the vault by one version. Every change is a new head in a signed chain — history stays reachable.

## What just happened

- **Identity** — an Ed25519 keypair generated locally; its public half identifies this device.
- **Vault secret** — a fresh symmetric key for `notes`, sealed to your device with X25519. Only keyholders can read the vault; anyone can mirror its ciphertext.
- **Content** — your file was encrypted and stored as a content-addressed blob; the (encrypted) directory tree references it.
- **Chain** — each save signs a new manifest that links to the previous one.

## Next steps

- Share a vault with another device or person: `zim vault shares add <vault> <key>`
- Mount a vault as a folder (FUSE builds): `zim mount add <vault> <path>`
- Keep a hub copy and browse from the web: `zim hub login`
- [Install]({{ '/docs/install/' | relative_url }}) for FUSE, updates, and service management.
