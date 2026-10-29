# Architecture

How Zim's implementation is divided and how its subsystems compose. Product
meaning and guarantees live in [`product/`](../product/index.md); reusable
engineering rules live in [`patterns/`](../patterns/index.md).

## Layers

Dependencies point inward:

```text
zim-hub/web -> zim-hub/wasm -> zim-api, zim-core, zim-crypto, zim-did

zim-desktop -> zim (lib: typed daemon client, init, hub login); spawns the zim binary as a sidecar
zim-desktop (webview) -> zim-hub/web (same SPA; `backend::desktop` over Tauri invoke)

zim-hub -> zim-peer, zim-api, zim-core, zim-crypto, zim-did
zim     -> zim-peer, zim-api, zim-core, zim-crypto, zim-did

zim-peer -> zim-core, zim-crypto, zim-did
zim-core -> zim-crypto, zim-did
zim-did  -> zim-crypto
```

- `zim-crypto` and `zim-did` own cryptographic and identity primitives.
- `zim-core` owns transport-independent vault and filesystem behavior.
- `zim-api` owns shared HTTP contracts and typed clients.
- `zim-peer` owns native storage, iroh transport, synchronization, and service
  lifecycle.
- `zim` and `zim-hub` compose those libraries into deployable processes.
- `zim-api`'s `desktop` feature is the shell↔SPA IPC contract (typed arguments, results and one error enum), compiled by both ends.
- `zim-desktop` is a Tauri shell, outside the root workspace. It runs no peer
  of its own: it connects to (or launches, as a detached sidecar) the `zim`
  daemon for `$ZIM_HOME` and proxies vault operations to its HTTP API, so the
  app and the CLI share one device identity.
- `zim-hub/wasm` and `zim-hub/web` keep plaintext vault operations in the
  browser.

## Subsystems

| Module | Purpose |
|---|---|
| [Vaults](vaults/index.md) | Data model, encrypted storage, history, sharing, and synchronization |

Add architecture by subsystem, not by crate. Crate boundaries matter only
where they enforce a dependency or runtime boundary.
