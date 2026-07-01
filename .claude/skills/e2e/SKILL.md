---
description: Run end-to-end dev environment tests (local p2p sync + web↔local sync)
allowed-tools:
  - Bash(./bin/dev)
  - Bash(./bin/dev *)
  - Bash(./bin/fuse-e2e)
  - Bash(./bin/fuse-e2e *)
  - Bash(./bin/minio *)
  - Bash(curl *)
  - Bash(docker exec zim-minio *)
  - Bash(podman exec zim-minio *)
  - Bash(tmux capture-pane *)
  - Bash(tmux has-session *)
  - Bash(tmux list-windows *)
  - Bash(sleep *)
  - Bash(echo *)
  - Read
  - Grep
  - Glob
---

Run end-to-end tests of the dev environment: peer-to-peer sync between local
daemons, and (with the hub) web↔local sync. See `docs/reference/development.md`
for the full `bin/dev` reference.

Dev nodes are **`alice`** and **`bob`** (from `bin/dev_/nodes.toml`), each a
`zim` daemon with its own `$ZIM_HOME` under `data/<nick>/`. The dev binary is
`target/debug/zim`, built once with `--features hub` (and `fuse` when opted in).

## IMPORTANT: sync timing & settling

P2P sync is **eventually consistent** — give it time:

- After `seed`, p2p discovery takes a few seconds; the seed step already waits
  for dialability, but cross-node reads may lag a moment. Re-check, don't panic.
- "No addressing information available" is **transient** (discovery in progress),
  not a failure.
- For web↔local: the daemons' address books are **settled by `zim hub peers
  sync`** (which `./bin/dev hub enroll` runs for you), NOT instantly at login.
  Until a daemon has synced the account roster, it won't recognise the web key /
  sibling devices. If web↔local sync looks stuck, re-run `hub peers sync` (or
  `./bin/dev hub enroll`) and wait.

## Track A — local p2p sync (no docker/hub)

Fastest signal that core sync works.

1. Clean start: `./bin/dev kill --force && ./bin/dev clean`
2. Start daemons: `./bin/dev run -b`
3. Wait for health: `./bin/dev status` (both `alice`/`bob` should be `up`), or
   poll `curl -sf http://127.0.0.1:17172/_status/livez`.
4. Seed vaults + shares: `./bin/dev seed`
   (alice creates `demo` + `notes`, drops content, shares `demo` with bob.)
5. Verify cross-node sync (give it a few seconds; retry if lagging):
   - `./bin/dev cli bob vaults list` → should include `demo`
   - `./bin/dev cli bob vault demo cat /readme.md` → alice's content
6. Round-trip the other way:
   - `echo "hi from bob" | ./bin/dev cli bob vault demo add /b.md`
   - `./bin/dev cli alice vault demo cat /b.md`
7. Check for real errors: `./bin/dev logs alice` / `./bin/dev logs bob`
   (ignore transient "No addressing information").

## Track B — full stack: web↔local sync (needs docker + confit)

8. One-shot bring-up: `./bin/dev --hub`
   (daemons → hub up [minio + real OAuth via confit] → seed → `hub enroll`,
   which writes each node's `hub-session.json` and runs `zim hub peers sync`.)
   Equivalently, step-by-step: `./bin/dev run -b`, `./bin/dev hub up`,
   `./bin/dev seed`, `./bin/dev hub enroll`.
9. Confirm enrollment settled: `./bin/dev cli alice hub peers ls` — the account
   roster should list the daemons (and the web key once minted), marked
   `✓ in book`.
10. Browser (manual): open `http://127.0.0.1:8080`, sign in as the seed user
    (`$ZIM_DEV_SEED_EMAIL`, default `al@krondor.org`), finish onboarding to mint
    the web key, open the vault tree. You should see the seeded vaults.
11. **web → local:** create/edit a file in the browser tree, then
    `./bin/dev cli alice vault <id> cat /<file>` — the hub announces the new head
    to shareholder daemons over iroh; they pull it.
12. **local → web:** `echo hi | ./bin/dev cli alice vault demo add /from-alice.md`,
    then refresh the web tree. (If it doesn't appear, re-run
    `./bin/dev cli alice hub peers sync` and wait — the roster settles sync.)
13. Verify blobs landed in minio: `./bin/minio status`, and
    `docker exec zim-minio mc ls local/zim-blobs/ | head` (or `podman exec` — the
    runtime `bin/minio` picked; container is `zim-minio`, bucket `zim-blobs`).

## FUSE filesystem tests

FUSE is opt-in. Two ways to exercise it:

- **Standalone harness (preferred):** `./bin/fuse-e2e` boots a `--features fuse`
  daemon, mounts a vault, and cross-checks every filesystem op (write/read/mkdir/
  rm/mv) against the vault via `zim vault … cat/ls`. `--sync` also runs the
  2-node sync flow; `--keep` leaves the mount up for inspection.
- **In the dev env:** `./bin/dev run -b --fuse` builds the daemons with mount
  support, then `zim mount add <vault> <mountpoint>` via the CLI.

**Availability** needs both (a) the platform lib — `/Library/Filesystems/macfuse.fs`
on macOS, `/dev/fuse` + libfuse on Linux — and (b) a daemon built with `--features
fuse`. If FUSE isn't available, report "FUSE tests skipped (not available)" —
that is **not** a failure. If it IS available but a mount op fails, that **is** a
failure.

## Report format

```
## E2E Test Results

### Daemon health
- alice: [up/down]
- bob:   [up/down]

### Local p2p sync (Track A)
- Seed applied (demo/notes created + shared): [yes/no]
- bob sees `demo`: [yes/no]
- bob reads alice's /readme.md: [yes/no]
- bob→alice round-trip (/b.md): [pass/fail]

### Web↔local sync (Track B — if run)
- Hub up (http://127.0.0.1:8080): [yes/no/skipped]
- Enrollment settled (`hub peers ls` shows roster ✓): [yes/no]
- Web signed in + web key minted: [yes/no/manual]
- web→local (browser edit visible via `cli … cat`): [pass/fail/skipped]
- local→web (daemon add visible in tree): [pass/fail/skipped]
- Blobs in minio (zim-blobs): [yes/no]

### FUSE (if available)
- FUSE available: [yes/no/skipped — not available]
- `./bin/fuse-e2e`: [pass/fail/skipped]

### Errors
[Real errors only — NOT transient "No addressing information available"]

### Summary
[PASS/FAIL] — [description]
```
