# Splitting `zim` (CLI + daemon) from `zim-hub`

**Stage:** Design
**Priority:** Medium
**Dependency:** `zim-api` contract stability

## Decisions (recorded 2026-10-01)

These are settled direction, not open questions:

- **`zim` and `zim-hub` are on different release cycles.** The CLI/daemon
  ships on crates.io + GitHub releases; the hub deploys continuously
  as a container image.
- **`zim-hub` becomes a private repo.** The hub is the hosted layer; it
  is not open-source infrastructure.
- **Two CLIs ship:** `zim` (daemon/vault CLI, public) and **`zh`** (hub
  CLI). The standing concern from the first discussion: *"the main
  thing would be getting the `zh` CLI to affect the `zim` CLI state."*
- **Until the split executes, the hub keeps shipping from this repo** —
  keep them together until the APIs are stable (earlier decision, still
  in force). The split is the destination, not the current state.
- **Hub-only dev config moves with the hub.** `confit.toml` (Google
  OAuth via the 1Password Environment) exists only to serve hub dev; it
  goes to the hub repo. The `confit export → .env.dev` idea is deferred
  to the split for the same reason — and so does removing the
  environment ID from this public repo's history-forward state.
- **Product framing:** `zim` = free, peer-to-peer, end-to-end-encrypted
  vaults. The hub = the hosted layer that makes it convenient (the
  GitHub-to-git analogy).

## What the split must respect (known constraints)

- **`zim-api` is the seam.** It holds the shared HTTP contract — daemon
  RPC, hub routes, JWT. It must be frozen/versioned before the repos
  separate; every cross-repo call goes through it.
- **The public crates are load-bearing.** `zim-crypto`, `zim-did`,
  `zim-core`, `zim-api`, `zim-peer`, `zim-cli` are published to
  crates.io. A private hub consumes them as ordinary dependencies, so
  **public-crate semver discipline is what keeps the hub buildable** —
  this is why the semver checks exist and why `zim-core`'s error-enum
  shape change was flagged for a minor bump.
- **The hub crate is three things:** the server, the `wasm/` browser
  SDK (`zim-hub-wasm`, `publish = false`), and the `web/` Yew SPA. It
  deploys via the GHCR image (`images.yml`); `release-plz` already treats
  the hub crates as `release = false`. The browser SDK builds on public
  `zim-core` for wasm32, so a private hub repo can own it.
- **How `zh` affects daemon state is already defined:** the daemon
  enrolls with the hub (`zim hub login`, device-code flow) and syncs the
  account roster (`zim hub peers sync`) by mutating its *own* state
  through the daemon's HTTP API — which lives in `zim-api` and is
  public. `zh` driving the local daemon uses that same API; no private
  channel is needed.
- **The e2e harness splits along the same seam.** Track A
  (daemon↔daemon, FUSE, forks, durability) is hub-free and stays with
  `zim`. Track B (web↔local through the hub) needs the hub.
- **The trust model is unchanged by the repo split.** The `did:web` hub
  is trusted for roster/app/escrow by design; E2EE covers content.
  Moving code between repos moves no trust boundary.

## Open — decisions still to make

1. **What `zh` is.** A separate binary for hub-side operations (account,
   devices, admin) vs. the existing `zim hub …` subcommands. Which
   `zim hub *` commands **stay in `zim`** — device pairing must, since
   the daemon enrolls itself.
2. **`zim-api` freeze criteria** and the hub↔daemon compatibility
   matrix: the hub pins a `zim-api` version; who bumps what, and how a
   daemon learns it's too old.
3. **Where Track B e2e lives:** in the hub repo consuming the published
   `zim-cli`, or in `zim` pulling the GHCR hub image.
4. **Release coordination** across two cadences — especially for
   changes that touch `zim-api`.

## When it becomes actionable

Create the jig/Linear epic from this page and link back here for the
product context. The trigger is `zim-api` reaching a shape both sides
are willing to freeze.
