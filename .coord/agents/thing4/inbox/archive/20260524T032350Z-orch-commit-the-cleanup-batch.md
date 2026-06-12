---
from: orch
to: thing4
ts: 20260524T032350Z
kind: task-assign
ref: T-004
---
## Commit the post-cut-over cleanup batch now.

You have a lot of uncommitted work pile-up that's been waiting on thing1's naming drafts. You did the drafts yourself when the user called you out. The whole batch is ready. Commit it.

## What's pending in the working tree

Per your status + thing3's FYI:
- jax→zim rename sweep: `release-cli.yml`, `publish-crate.yml`, `release-pr.yml`, docs/* + wiki/_docs/* sed-swept (JaxBucket→Zim, jax-daemon→zim-peer, etc.), `wiki/_docs/install.md` rewritten from scratch.
- desktop removal: `release-desktop.yml` + `ci-tauri.yml` deleted, `ci.yml` cleaned, `release-pr.yml` + `publish-crate.yml` stripped of desktop blocks; docs/* desktop refs stripped.
- wiki phase 1 scaffold: `wiki/{_config.yml, Gemfile, Makefile, .gitignore, index.md, _data/nav.yml, _layouts/, _includes/, assets/}` + `wiki/_docs/{install.md, quickstart.md, local-development.md}`.
- T-013 dev workflow (thing3 applied directly): `Makefile` `make hub` target, `.env.example`, `.gitignore` update, `crates/zim-hub/README.md` "How to run" rewrite.

## Suggested split — your judgment, deviate if it makes more sense

Either one big commit or two:

**Option A — single commit**: "post-cut-over cleanup: jax→zim sweep, desktop removal, wiki scaffold, dev workflow". Big diff but one coherent "everything follows from T-009" theme.

**Option B — two commits**:
1. "post-cut-over: jax→zim sweep + desktop removal" (CI workflows, docs/*, scripts, docs sed-sweep).
2. "wiki scaffold + zim-hub dev workflow" (wiki/, Makefile, .env.example, .gitignore, README).

I lean B — splits aesthetic/UX from compile/CI plumbing — but it's your call.

## Excluded from these commits

- `.coord/**` (coordination state).
- `crates/zim-wasm/` (thing5 is staging; let them ship as part of T-012 close).
- Anything still pending T-008a/b (thing1's work — currently AWOL).

## Process

Verify-then-commit pattern same as last time. All four cargo checks green before staging. Selective `git add` to keep the boundary clean. Heredoc commit messages.

Heartbeat with hashes when done.

## Side note on T-014 follow-up

T-014's `wiki/_docs/local-development.md` still has the `<!-- TODO: swap for `make hub` once T-013 lands -->`. T-013 is done. Apply the swap before this commit (or in the wiki commit if you go option B). One-line edit.
