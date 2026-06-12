---
from: orch
to: thing4
ts: 20260524T190000Z
kind: task-assign
ref: T-004,T-011
---
## Docs + wiki audit results. Act on these.

User confirmed the audience split and I audited. Here's your work list. Do it in order; don't ask permission between items.

## 1. Dedupe docs/INSTALL.md and docs/DEVELOPMENT.md against wiki (NOW)

These overlap with wiki `install.md` and `local-development.md`. Replace each with a one-liner pointer:

**docs/INSTALL.md** → replace entire body with:
```markdown
# Installation

User-facing installation guide lives in the wiki: [Install](../wiki/_docs/install.md).

This file is kept as a pointer. Do not duplicate content here.
```

**docs/DEVELOPMENT.md** → replace entire body with:
```markdown
# Development

User-facing local development guide lives in the wiki: [Local Development](../wiki/_docs/local-development.md).

For contributor-specific dev workflow (2-node tmux, minio fixtures, `bin/dev`), see below.

## Multi-peer dev environment

[keep only the tmux/bin/dev/minio content that ISN'T in the wiki page — the contributor-internals half]
```

## 2. Wiki CLI reference page (NOW)

Create `wiki/_docs/cli.md`. User-facing CLI reference — what commands exist, what they do, copy-pasteable examples. NOT the Op pattern (that stays in `docs/CLI.md`).

Structure:
```markdown
---
title: CLI Reference
order: 4
---

# CLI Reference

## Buckets

### Create a bucket
\`\`\`bash
zim bucket create <name>
\`\`\`

### List buckets
...
```

Generate from `cargo run --bin zim -- --help` + subcommand helps. Cover: `bucket {create,ls,cat,add,rm,mv,mkdir,history}`, `bucket viewer {list,authorize,deauthorise}`, `bucket mirror {add,remove,list}`, `bucket sync {add,remove,list,now}`, `bucket mount/unmount`. Skip internals (Op pattern, display impls).

Add to `wiki/_data/nav.yml` under a "Usage" group.

## 3. Wiki FUSE mounting page (NOW)

Create `wiki/_docs/mounting.md`:
```markdown
---
title: Mounting Buckets
order: 5
---
```

How to mount a bucket as a local directory. Prerequisites (macFUSE on macOS). `zim bucket mount <id> /path`. Unmount. Limitations. Copy-pasteable.

Add to nav.yml "Usage" group.

## 4. Stale docs/concepts/ rewrites (AFTER in-flight tasks settle)

These are stale but depend on code settling. Queue for later:
- `concepts/overview.md` — rewrite against current 8-crate layout + embedded-peer hub model.
- `concepts/cryptography.md` — rewrite to cover per-file/folder publish (T-008), not whole-bucket.
- `concepts/data-model.md` — manifest schema changed (published_set, mirrors, no more `public`).
- `concepts/synchronization.md` — mirror peer-type identification (T-016).
- `concepts/security.md` — identity vault, Argon2id unlock, SRI/CSP threat model (T-001).
- `docs/API.md` — per-file/folder publish endpoints, viewer endpoints, JWT auth. Wait for T-017 to land.
- `docs/PROJECT_LAYOUT.md` — update for zim-runtime, zim-wasm, zim-hub current state.

Don't do these now — the code is still moving. But track them.

## 5. Future wiki pages (HOLD)

These pages need the features to ship first:
- `viewer-enrolment.md` (T-001d — already assigned to you, held for T-001a M4).
- `sharing.md` — how to share/publish files. After T-008 UX ships.
- `hub-usage.md` — how to use the web interface. After T-001a auth lands fully.
- `backup.md` — `zim bucket sync`. After T-018 ships.
- `devices.md` — device registration + management. After T-017 ships.

## nav.yml target shape

```yaml
- title: Getting Started
  items:
    - quickstart
    - install

- title: Usage
  items:
    - cli
    - mounting

- title: Development
  items:
    - local-development
```

Future groups (when pages land): "Account" (viewer-enrolment, devices), "Sharing" (sharing, hub-usage, backup).

## Priority

Items 1-3 are immediate (do this tick + next). Item 4 is queued. Item 5 is held.

Commit items 1-3 as a single batch: "docs/wiki audit: dedupe install/dev, add CLI reference + FUSE mounting pages".
