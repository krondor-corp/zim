---
from: thing4
to: orch
ts: 20260524T020932Z
kind: status-request
ref: T-004
---

Heartbeat + scope-extension request.

## Done since last update

Applied thing3's §E `docs/**` desktop-removal edits (his draft at `.coord/agents/thing3/drafts/desktop-removal-list.md`). Five files cleaned of Tauri/desktop references:

- `docs/INSTALL.md` — Desktop App section replaced with single-binary stub
- `docs/PATTERNS.md` — Tauri Desktop Patterns section deleted
- `docs/PROJECT_LAYOUT.md` — "(used by Tauri)" qualifier dropped, `crates/desktop` section deleted, `release-desktop.yml` dropped from workflows list
- `docs/concepts/index.md` — Daemon-as-Library paragraph rewritten to drop Tauri example
- `docs/concepts/fuse-architecture.md` — "and desktop app" phrase removed

Verified clean via grep. No commits.

Reply sent to thing3.

## Scope extension request

thing3's draft also lists desktop-removal edits OUTSIDE my current `files_owned` (`.git/**`, `.coord/**`, `docs/**`, `wiki/**`):

1. **`README.md`** — lines 18–24 (Desktop App section), 42–49 (download table), 75 (`jax-desktop` crate entry). Replace top section with a one-liner pointing to the headless daemon + future zim-hub.
2. **`CLAUDE.md`** — line 31 crate-tree comment dropping `desktop/`.
3. **`bin/{test,check,build}`** — drop `--exclude jax-desktop` from each.
4. **`.github/workflows/`** — 5 files:
   - `release-desktop.yml` — **delete**
   - `ci-tauri.yml` — **delete**
   - `ci.yml` — drop `desktop-ci` job and `needs: [rust-ci, desktop-ci]`
   - `release-pr.yml` — drop every `jax-desktop` block
   - `publish-crate.yml` — drop `DESKTOP_VERSION` and the desktop tag entry
   - `release-cli.yml` — review (likely safe)

thing3 wrote: "no current owner; you might be the closest fit since you already own repo-level files." I'd argue the same — they're commit-adjacent housekeeping that fits the "GitOps" half of my role.

**Three options:**

a. **Extend my ROLE** to add `README.md`, `CLAUDE.md`, `bin/**`, `.github/**` (or a narrower subset). Cleanest going forward — these are all "commit infrastructure" files I touch incidentally.

b. **One-time delegation** of just the desktop-removal edits, no ROLE change. I make a single pass, no future writes without re-authorization.

c. **Punt to another worker** if you prefer the GitOps surface stay narrow.

My recommendation: **(a)**, narrowing to `README.md`, `CLAUDE.md`, `bin/**`, `.github/**`. These naturally belong with the docs/git-ops responsibility.

## Holding pattern

User instruction is "wake up every 15 minutes to monitor inbox and keep chugging along." I'm scheduling that loop now. Each wake:

1. Poll inbox + broadcasts + tasks/open
2. Apply any in-scope work that's safely actionable
3. Surface anything ambiguous or out-of-scope to you
4. Heartbeat STATUS

If you need to halt me, drop a message in my inbox and I'll see it next wake.

Still uncommitted: phase-1+2 wiki, docs/index.md pointer, 5 desktop-removal edits, all `.coord` updates.
