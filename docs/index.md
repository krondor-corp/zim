# Documentation Index

Central hub for contributor and agent documentation. Read the
[documentation guidelines](./_guidelines/index.md) before adding a page.

> End-user help lives in [`web/`](../web/). The files here are for people and
> agents working on Zim.

## Quick Start

See [`dx/getting-started.md`](dx/getting-started.md).

## Structure

```
docs/
├── _guidelines/             # Where docs belong and how the public site works
├── product/                 # What Zim is and does
├── architecture/            # How subsystems are built
├── patterns/                # Cross-cutting rules and contracts
├── dx/                      # Local development and contribution
├── devops/                  # Releases and operations
└── research/                # Non-authoritative design investigations
```

## Modules

| Module | Purpose |
|---|---|
| [Product](product/index.md) | Capabilities, guarantees, identity, security, and roadmap |
| [Architecture](architecture/index.md) | Subsystem boundaries, relationships, flows, and invariants |
| [Patterns](patterns/index.md) | Cross-cutting conventions, CLI boundaries, and HTTP contracts |
| [Developer Experience](dx/index.md) | Setup, local development, debugging, testing, and contribution |
| [DevOps](devops/index.md) | Release and operational workflows |
| [Guidelines](_guidelines/index.md) | Documentation placement and maintenance rules |

## For AI Agents

Read [`dx/getting-started.md`](dx/getting-started.md) and
[`patterns/conventions.md`](patterns/conventions.md) first, then the relevant module index. Keep
`.claude/skills/` synchronized when documented commands or workflows change.
