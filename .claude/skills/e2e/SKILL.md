---
description: Run end-to-end dev environment tests (fixtures, cross-node sync, FUSE, hub/web)
allowed-tools:
  - Bash(./bin/dev)
  - Bash(./bin/dev *)
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

Run end-to-end tests of the dev environment to verify fixtures, cross-node
sync, FUSE, and (optionally) web↔local sync through the hub. See
`docs/dx/local.md` for the full `bin/dev` reference.

Dev nodes are **`alice`** and **`bob`** (from `bin/dev_/nodes.toml`; ports per
its band map — daemons 1717x, infra 1718x, hub 1719x), each a `zim` daemon
with its own `$ZIM_HOME` under `data/<nick>/`.

**Expected end state is documented in `bin/dev_/fixtures.toml`** — see the
"EXPECTED END STATE" comment at the end of that file for what to verify.

## IMPORTANT: Sync Timing

**Be patient with sync.** P2P discovery and sync take time in local dev:
- "No addressing information available" is **transient** (discovery in
  progress), not a failure.
- Cross-node reads may lag the fixture apply by seconds to ~a minute; the
  reconcile sweep re-announces, so re-check before declaring failure.
- For web↔local: daemon address books settle via `zim hub peers sync` (run by
  `./bin/dev hub enroll`), NOT instantly at login. If web↔local looks stuck,
  re-run enroll and wait.

## E2E Test Flow (Track A — local p2p, no docker)

**Fast path: `make e2e`** runs the `zim-e2e` crate: a hermetic one-shot
with a PASS/FAIL exit code — its own daemons on the 1722x band (fresh
homes under `data/e2e/`, the interactive env is untouched), fixtures
through the real CLI, and poll-based cross-node verification (peers
cross-introduced directly, so no DHT wait). Use the manual steps below
when you need to inspect intermediate state in the *interactive* env;
the checks are the same.


1. `./bin/dev kill --force && ./bin/dev clean` — clean start
2. `ZIM_DEV_FUSE=1 ./bin/dev run -b` — start nodes (FUSE feature on)
3. Wait for health: `./bin/dev status` (or poll
   `curl -sf http://127.0.0.1:17172/_status/livez`)
4. **FUSE detection**: `./bin/dev fuse-check` — reports whether FUSE fixtures
   will run
5. Apply fixtures: `ZIM_DEV_FUSE=1 ./bin/dev seed` — peer plumbing, then every
   declarative fixture in `bin/dev_/fixtures.toml` (vaults, files, shares,
   mv, FUSE ops). **Any `FAILED` line here is a real failure.**
6. Verify on alice: `./bin/dev cli alice vault list`,
   `./bin/dev cli alice vault cat demo /guide.md`
7. **Wait for sync**, then verify on bob:
   - `./bin/dev cli bob vault list` → includes `demo`
   - `./bin/dev cli bob vault cat demo /readme.md` → alice's content
8. Round-trip the other way:
   - `echo "hi from bob" | ./bin/dev cli bob vault add demo /b.md`
   - `./bin/dev cli alice vault cat demo /b.md`
9. Check for **real** errors: `./bin/dev logs alice` / `./bin/dev logs bob`
   (ignore transient "No addressing information").

## FUSE Filesystem Tests

FUSE operations run automatically inside the fixture apply (step 5) as
declarative fixture types in `fixtures.toml`:

- `mount` / `mount_verify` / `unmount` — lifecycle
- `fuse_ls`, `fuse_read`, `fuse_write`, `fuse_mv`, `fuse_mv_in`,
  `fuse_mv_out`, `fuse_rm` — filesystem ops through the mountpoint
- `vault_read` — cross-checks a FUSE write landed in the encrypted vault
  (API↔FUSE coherence)

**Availability needs both** (a) the platform lib (`/Library/Filesystems/macfuse.fs`
on macOS, `/dev/fuse` on Linux) and (b) a daemon built with the fuse feature
(`_status/version` → `build_features`). `./bin/dev fuse-check` reports both.

**Reporting rules:**
- FUSE unavailable → fixtures auto-skip → report "FUSE tests skipped (not
  available)" — **NOT a failure**
- FUSE available but a fixture fails → **IS a failure**
- The report must state whether FUSE fixtures ran or were skipped

## Track B — full stack: web↔local sync (needs docker + confit)

10. One-shot bring-up: `ZIM_DEV_FUSE=1 ./bin/dev --hub`
    (daemons → hub up [minio + real OAuth via confit] → seed/fixtures →
    `hub enroll`). Step-by-step equivalent: `run -b`, `hub up`, `seed`,
    `hub enroll`.
11. Confirm enrollment settled: `./bin/dev cli alice hub peers ls` — the
    account roster lists the daemons (and web key once minted), `✓ in book`.
12. Browser (manual): open `http://127.0.0.1:17190`, sign in as the seed user
    (`$ZIM_DEV_SEED_EMAIL`, default `al@krondor.org`), mint the web key, open
    the vault tree — seeded vaults should appear.
13. **web → local**: edit a file in the browser tree, then
    `./bin/dev cli alice vault cat <vault> /<file>` — the hub announces the
    head to shareholder daemons over iroh.
14. **local → web**: `echo hi | ./bin/dev cli alice vault add demo /from-alice.md`,
    refresh the tree. (Stuck? `./bin/dev cli alice hub peers sync`, wait.)
15. Verify blobs in minio: `./bin/minio status`, then
    `podman exec zim-minio mc ls local/zim-blobs/` (or `docker exec` — whichever
    runtime `bin/minio` picked; container `zim-minio`, bucket `zim-blobs`).

## Report Format

```
## E2E Test Results

### Node Health
- alice: [up/down]
- bob:   [up/down]

### Fixtures (./bin/dev seed)
- Applied cleanly: [yes/no — list any FAILED lines]
- FUSE fixtures: [ran/skipped — not available]

### Cross-Node Sync
- bob sees `demo`: [yes/no]
- bob reads alice's /readme.md: [yes/no]
- bob→alice round-trip (/b.md): [pass/fail]

### Web↔Local Sync (Track B — if run)
- Hub up (http://127.0.0.1:17190): [yes/no/skipped]
- Enrollment settled (`hub peers ls` roster ✓): [yes/no]
- Web signed in + web key minted: [yes/no/manual]
- web→local: [pass/fail/skipped]
- local→web: [pass/fail/skipped]
- Blobs in minio (zim-blobs): [yes/no]

### Errors
[REAL errors only — NOT transient "No addressing information available"]

### Summary
[PASS/FAIL] — [description]
```
