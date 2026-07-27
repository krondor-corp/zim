# zim-hub

Always-on rendezvous server for Zim: a **ciphertext mirror + key-escrow service** for browser-resident verification methods. The browser holds the keys; the hub holds the bytes. End-to-end encryption is preserved.

## What this is

zim-hub embeds a `zim` peer in-process and serves three things over HTTP:

1. **Ciphertext mirror** — registered as a vault `Relay`, the hub holds every encrypted blob + log entry referenced by a vault's chain. It cannot decrypt anything.
2. **DID document hosting** — serves the user's `did:web` document at `/u/{user}/.well-known/did.json`. Updates require a signature from a current controller in the previous doc; the hub does not authenticate writes by any other means.
3. **Key escrow** — passphrase-wrapped browser identity keys, indexed by DID verification-method fragment. The hub never sees the passphrase or the unwrapped key.

The browser uses zim-wasm to fetch ciphertext + manifest blobs, decrypt them against the unwrapped web identity, and render the vault tree + history client-side.

See [`docs/research/hub-revival.md`](../../docs/research/hub-revival.md) for the architecture and phase plan.

## Status

Phase 1 (mechanical revival): crate moved into the workspace, deps swapped against the current `zim` lib surface, obsolete OAuth scaffolding deleted. Ciphertext-serving HTTP endpoints (`/api/v0/vaults/{id}/{head,log,blob/...}`) are wired but blocked on the in-flight DID refactor in `zim-core` / `zim-vault`.

Phase 2 onward (sync verification, DID hosting, escrow service, browse + history UI) lives behind the DID infrastructure landing.

## How to run

```
make hub
```

Starts the hub on `http://localhost:17190` in the dev tmux session (window `hub`), with `cargo watch` over the hub + shared sync crates. `./bin/dev hub up` is the same thing; `cargo run -p zim-hub` runs it bare in the foreground (build the SPA first: `make build-web`).

Defaults:
- `ZIM_HUB_LISTEN=127.0.0.1:17190` — HTTP bind address
- `ZIM_HUB_HOME=./data/zim-hub` — data directory (peer identity, vault log, blobs)
- `ZIM_HUB_LOG=info`

## Endpoints (Phase 1)

```
GET  /                              -- minimal landing: lists every vault on the hub
GET  /_status/{livez,readyz,version}
GET  /api/v0/vaults/{vault_id}/head
GET  /api/v0/vaults/{vault_id}/log?from=N&limit=M
GET  /api/v0/vaults/{vault_id}/blob/{hash}
GET  /static/*path
```

All `/api` endpoints serve raw bytes or public log metadata. No decryption happens server-side.

## Layout

```
crates/zim-hub/
├── Cargo.toml
├── README.md
├── Makefile
├── build.rs           — SRI hashes for vendored JS
├── src/
│   ├── main.rs        — boot ServiceState, spawn peer + http
│   ├── lib.rs         — module declarations + re-exports
│   ├── config.rs      — ZIM_HUB_LISTEN / ZIM_HUB_HOME / ZIM_HUB_LOG
│   ├── state.rs       — AppState { listen_address, service: zim::ServiceState }
│   ├── errors.rs
│   ├── sri.rs         — generated SRI constants
│   └── http/
│       ├── mod.rs     — Router builder + Service impl
│       ├── api/v0/vaults/  — ciphertext + log endpoints (head, log, blob)
│       ├── health/    — livez / readyz / version
│       ├── html/      — index page + static asset serving
│       └── sse/       — reserved for future merge-fragment streams
├── templates/         — Askama HTML templates
└── static/            — vendored JS + CSS; vendor/zim-wasm/ is the hot path
```
