# Documentation Index

Central hub for project documentation. AI agents should read this first.

> User-facing documentation lives in [`wiki/`](../wiki/) (Jekyll). The files here cover contributor and agent process — patterns, conventions, architecture, reference.

## Quick Start

See [getting-started.md](./getting-started.md).

## Structure

```
docs/
├── getting-started.md       # Build, run, orient
├── concepts/                # What and why: data model, crypto, sync, access, identity
├── architecture/            # How it's built: project layout, FUSE, runtime
├── reference/               # Lookup: HTTP API, CLI, debugging, dev environment
├── deployment/              # Ship: release process, infra (future)
├── PATTERNS.md              # Coding conventions
├── CONTRIBUTING.md          # Contribution workflow
├── SUCCESS_CRITERIA.md      # CI gates
├── CRATES.md                # Crate layout and dependency graph
└── ISSUES.md                # Issue/ticket conventions
```

## Concepts

| Document | Purpose |
|----------|---------|
| [concepts/overview.md](./concepts/overview.md) | Architecture overview, data model, access control |
| [concepts/data-model.md](./concepts/data-model.md) | Bucket, manifest, nodes, links |
| [concepts/cryptography.md](./concepts/cryptography.md) | Ed25519, X25519, ChaCha20-Poly1305 |
| [concepts/synchronization.md](./concepts/synchronization.md) | Peer sync protocol |
| [concepts/acceptance-and-sharing.md](./concepts/acceptance-and-sharing.md) | `AcceptPolicy` hook, recipient routing, share population |
| [concepts/security.md](./concepts/security.md) | Threat model, trust boundaries |
| [concepts/access-model.md](./concepts/access-model.md) | Shares, mirrors, publication, relay |
| [concepts/identity.md](./concepts/identity.md) | Web-key vault, Google auth, Argon2id |
| [concepts/conflict-resolution.md](./concepts/conflict-resolution.md) | CRDT path ops, merge strategies |

## Architecture

| Document | Purpose |
|----------|---------|
| [architecture/project-layout.md](./architecture/project-layout.md) | Crate structure, module map |
| [architecture/fuse.md](./architecture/fuse.md) | FUSE filesystem integration |

## Reference

| Document | Purpose |
|----------|---------|
| [reference/api.md](./reference/api.md) | HTTP API endpoints |
| [reference/cli.md](./reference/cli.md) | CLI commands (Op pattern) |
| [reference/debugging.md](./reference/debugging.md) | Log inspection, API testing |
| [reference/development.md](./reference/development.md) | Dev environment, tmux, 2-node setup |

## Deployment

| Document | Purpose |
|----------|---------|
| [deployment/release.md](./deployment/release.md) | Release process, cargo-smart-release, CI |

## Process

| Document | Purpose |
|----------|---------|
| [PATTERNS.md](./PATTERNS.md) | Coding conventions and patterns |
| [CONTRIBUTING.md](./CONTRIBUTING.md) | How to contribute |
| [SUCCESS_CRITERIA.md](./SUCCESS_CRITERIA.md) | CI checks that must pass |
| [CRATES.md](./CRATES.md) | Crate layout, naming, dependencies |
| [ISSUES.md](./ISSUES.md) | Issue and ticket conventions |

## For AI Agents

You are an autonomous coding agent working on a focused task. Read [getting-started.md](./getting-started.md) and [PATTERNS.md](./PATTERNS.md) first, then the relevant concept/reference page for your task.
