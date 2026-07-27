---
title: Quickstart
order: 2
---

From a fresh shell to a running daemon with one bucket in four commands.

## 1. Install the CLI

```bash
cargo install zim-peer
```

This installs the `zim` binary. See [Install]({{ '/docs/install/' | relative_url }}) for system-package, source, and FUSE variants.

## 2. Initialize this device

```bash
zim init
```

This creates a local state directory with:

- `config.toml` — daemon configuration
- `secret.pem` — your Ed25519 identity (back this up, do not commit it)
- a SQLite database for bucket metadata
- `blobs/` — encrypted blob storage

The public half of `secret.pem` is your Node ID.

## 3. Start the daemon

```bash
zim daemon
```

The daemon starts:

- HTTP API on `http://localhost:3000`
- Web UI on `http://localhost:8080`
- An iroh P2P endpoint that joins the DHT

Keep the daemon running. The CLI in another shell talks to the same daemon over the HTTP API.

## 4. Create and populate a bucket

```bash
zim bucket create my-bucket
zim bucket add my-bucket ./README.md
zim bucket ls my-bucket
```

The first command writes the genesis manifest (with your share encrypting the bucket secret). The second encrypts the file and appends a new manifest version. The third lists the bucket's root directory.

## What just happened

- **Identity**: An Ed25519 keypair was generated locally. Its public half is your Node ID.
- **Bucket secret**: A 256-bit ChaCha20-Poly1305 key was generated for `my-bucket`. It was wrapped with X25519 ECDH + AES-KW against your identity key and stored as your share inside the manifest.
- **File**: `README.md` was encrypted with its own per-file secret. The ciphertext was stored as a content-addressed blob. The bucket's directory tree (also encrypted) was updated to reference the new blob.
- **Manifest chain**: Each operation advances the bucket by one version. Old versions remain reachable; the chain is the audit trail.

## Next steps

- [Install]({{ '/docs/install/' | relative_url }}) for platform-specific notes, FUSE setup, and running as a background service.
