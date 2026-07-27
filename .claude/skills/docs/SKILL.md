---
description: Navigate Zim contributor and end-user documentation. Use when locating project guidance or deciding where documentation belongs.
allowed-tools:
  - Read
  - Glob
  - Grep
---

Navigate and maintain Zim documentation without mixing its two audiences.

## Documentation Homes

| Path | Audience | Content |
|---|---|---|
| `docs/` | Contributors and agents | Product, architecture, patterns, process, and deployment |
| `web/` | End users | Installation, configuration, and product workflows |

Read `docs/_guidelines/index.md` before creating or moving documentation. Read
`docs/_guidelines/web.md` before changing the public site.

## Common Lookups

- Product and security model: `docs/product/index.md`
- Subsystem implementation: `docs/architecture/index.md`
- Browser and WASM architecture: `docs/ui/index.md`
- Conventions, CLI, and HTTP contracts: `docs/patterns/index.md`
- Build, local development, and debugging: `docs/dx/index.md`
- Contribution and CI workflow: `docs/dx/contributing.md`, `docs/patterns/success-criteria.md`
- Deferred product direction: `docs/product/roadmap/index.md`
- Active work and status: Linear via `/issues`
- Releases and operations: `docs/devops/index.md`
- End-user instructions: `web/_docs/`, ordered by `web/_data/nav.yml`

## Workflow

1. Start at `docs/index.md` or the relevant module index.
2. Read focused pages and linked source only as needed.
3. Treat `docs/research/` as investigation, not shipped behavior.
4. When behavior changes, update the canonical page and its navigation entry.
5. If a workflow used by `.claude/skills/` changes, update that skill too.

When asked about a topic, find and summarize the relevant canonical documents.
When asked where new content belongs, apply the audience and module rules above.
