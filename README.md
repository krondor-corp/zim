# Zim

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**End-to-end encrypted peer-to-peer storage.** Built on iroh-blobs with ChaCha20-Poly1305 content encryption and X25519 secret sharing.

> **SECURITY DISCLAIMER**
>
> This software has NOT been audited by security professionals and is NOT production-ready. The cryptographic implementation and protocol design have not undergone formal security review. Do not use this software to protect sensitive, confidential, or production data. Use at your own risk.

## Overview

Zim organizes data into **buckets** — encrypted containers that sync directly between authorized devices. Each bucket has its own 256-bit secret; per-file and per-directory keys are split into shares and wrapped to each device's public key. No server holds plaintext.

## Install

```bash
# From crates.io
cargo install zim-peer

# Or via the install script
curl -fsSL https://raw.githubusercontent.com/zim/zim/main/install.sh | sh
```

See [docs/INSTALL.md](docs/INSTALL.md) for system-package and source builds.

## Quick start

```bash
zim init                          # one-time: generate identity + state dir
zim daemon                        # foreground; ctrl-c to stop
zim bucket create my-bucket       # in another shell
zim bucket add my-bucket ./file.txt
zim bucket ls my-bucket
```

## Workspace layout

| Crate | Description |
|-------|-------------|
| [`zim-crypto`](crates/zim-crypto/) | Ed25519/X25519 keys, ChaCha20-Poly1305, secret sharing |
| [`zim-store`](crates/zim-store/) | Content-addressed blob storage (SQLite + S3/MinIO/local) |
| [`zim-fs`](crates/zim-fs/) | Filesystem: manifest, nodes, CRDT path ops, conflict resolution |
| [`zim-protocol`](crates/zim-protocol/) | Wire protocol: peer messaging, sync jobs, append-only bucket log |
| [`zim-peer`](crates/zim-peer/) | System daemon binary (`zim`) + HTTP API + FUSE + database |
| [`zim-hub`](crates/zim-hub/) | Read-only web mirror gateway |
| [`zim-wasm`](crates/zim-wasm/) | Browser-side WASM client for the hub |

See [docs/CRATES.md](docs/CRATES.md) for the dependency graph and module conventions.

## Documentation

- **Users** — [wiki/](wiki/) (Jekyll-built; serve locally with `cd wiki && bundle exec jekyll serve`).
- **Contributors** — [docs/](docs/):
  - [PROJECT_LAYOUT.md](docs/PROJECT_LAYOUT.md) — crate-by-crate module map
  - [PATTERNS.md](docs/PATTERNS.md) — Rust conventions
  - [CONTRIBUTING.md](docs/CONTRIBUTING.md) — workflow, commits, tests
  - [DEVELOPMENT.md](docs/DEVELOPMENT.md) — local dev environment
  - [CLI.md](docs/CLI.md) — Op pattern, formatting boundary
  - [API.md](docs/API.md) — HTTP API reference
  - [RELEASE.md](docs/RELEASE.md) — release process
  - [concepts/](docs/concepts/) — data model, crypto, sync, conflict resolution, security, FUSE

## License

MIT — see [LICENSE](LICENSE).

## Built with

- [iroh](https://iroh.computer/) — P2P networking
- [Rust](https://www.rust-lang.org/) — systems programming
- [DAG-CBOR](https://ipld.io/) — content-addressed serialization
