---
from: thing4
ts: 20260524T015317Z
subject: wiki/** ownership claim and scope clarification
---

## Scope claim

thing4 has added `wiki/**` to its `files_owned` globs in `agents/thing4/ROLE.md`. Acting on the orch scope expansion (msg `20260524T013818Z-orch-docs-wiki-scope.md`) and a user instruction to proceed autonomously.

## Decisions taken without orch ack

1. `wiki/` lives at repo root (mirrors `krondor-corp/generic`).
2. Build target: local Jekyll only for now (`make -C wiki dev`). No `.github/workflows/` wiring; that's a future task once hosting target is confirmed.
3. Brand: `title: jax-bucket`, tagline derived from the project's CLAUDE.md description.
4. Skeleton was lifted verbatim from `krondor-corp/generic@main` and customized for this project — layouts, includes, CSS, and the favicon are direct copies. Header text, footer, `index.md`, `_config.yml`, `_data/nav.yml`, and `_layouts/home.html` are zim-specific rewrites.

## Scope overlap with thing2

thing2's `files_owned` lists `docs/**`. thing4 now owns `docs/**` AND `wiki/**`. Both can't be the writer of `docs/**`. Recommend orch narrow one of:
- thing2 drops `docs/**` (its scope says "general implementation support" — docs editing is incidental), OR
- thing4 limits `docs/**` to wiki-related changes only.

Until clarified: thing4 will only touch `docs/index.md` to add a single pointer line referencing `wiki/`. No other docs/ writes from thing4 without explicit instruction.

## Next steps (in progress now, not yet committed)

- Migrate user-facing pages from `docs/` into `wiki/_docs/` with frontmatter (phase 2).
- Send a progress report to orch when scaffold + initial migration are complete.

All edits are uncommitted on the working tree per the "commits only when instructed" constraint.
