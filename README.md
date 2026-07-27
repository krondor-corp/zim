# Zim

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**End-to-end encrypted peer-to-peer storage.** Built on iroh-blobs with ChaCha20-Poly1305 content encryption and X25519 secret sharing.

> **SECURITY DISCLAIMER**
>
> This software has NOT been audited by security professionals and is NOT production-ready. The cryptographic implementation and protocol design have not undergone formal security review. Do not use this software to protect sensitive, confidential, or production data. Use at your own risk.

## Overview

Zim organizes data into **vaults**: encrypted, versioned filesystems that synchronize between authorized devices. Hubs can retain and relay ciphertext without becoming vault shareholders.

## Install

```bash
# From crates.io
cargo install zim

# Or via the install script
curl -fsSL https://raw.githubusercontent.com/zim/zim/main/install.sh | sh
```

See [source installation](docs/dx/install.md) for system-package and source builds.

## Quick start

```bash
zim init                           # one-time: generate identity + state dir
zim daemon run                     # foreground; ctrl-c to stop
zim vault create my-vault          # in another shell
zim vault my-vault add /file.txt < ./file.txt
zim vault my-vault ls /
```

## Workspace layout

| Crate | Description |
|-------|-------------|
| [`zim-crypto`](crates/zim-crypto/) | Ed25519/X25519 keys, ChaCha20-Poly1305, secret sharing |
| [`zim-did`](crates/zim-did/) | `did:key` and `did:web` identities and resolution |
| [`zim-core`](crates/zim-core/) | Vault data model, filesystem, and linked data |
| [`zim-api`](crates/zim-api/) | Shared HTTP contracts and typed clients |
| [`zim-peer`](crates/zim-peer/) | Peer sync, storage, iroh transport, and runtime |
| [`zim`](crates/zim/) | Daemon, CLI, HTTP API, and FUSE mounts |
| [`zim-hub`](crates/zim-hub/) | Ciphertext mirror, web gateway, browser SDK, and web app |

See [architecture](docs/architecture/index.md) for dependency direction and subsystem boundaries.

## Documentation

- **Users** — [web/](web/) (Jekyll-built; serve locally with `make -C web dev`).
- **Contributors** — [docs/](docs/):
  - [Documentation index](docs/index.md) — contributor documentation map
  - [Product](docs/product/) — vault capabilities, cryptography, access, identity, and security
  - [Architecture](docs/architecture/) — subsystem boundaries, data relationships, and flows
  - [Patterns](docs/patterns/) — Rust conventions, CLI boundaries, and HTTP contracts
  - [Developer experience](docs/dx/) — setup, local development, and contribution
  - [UI](docs/ui/) — browser and WASM architecture
  - [DevOps](docs/devops/) — release and operational workflows

## License

MIT — see [LICENSE](LICENSE).

## Built with

- [iroh](https://iroh.computer/) — P2P networking
- [Rust](https://www.rust-lang.org/) — systems programming
- [DAG-CBOR](https://ipld.io/) — content-addressed serialization
