## File formats

All files are Markdown with YAML frontmatter. Body is freeform markdown with real newlines.

### Message

Path: `agents/<to>/inbox/<ts>-<from>-<subject>.md`

```markdown
---
from: orch
to: worker-auth
ts: 20260413T100000Z
kind: task-assign | status-request | reply | fyi
ref: T-017
---
Body here. Be concrete. State what you want and by when.
```

### Task

Path: `tasks/open/<id>.md`

```markdown
---
id: T-017
title: Add OAuth state validation
created_by: orch
created_at: 20260413T100000Z
assignee: null
files_expected:
  - crates/app/src/http/auth/google/callback.rs
priority: normal
---
## Goal
<one paragraph>

## Acceptance
- [ ] criterion 1
- [ ] criterion 2

## Out of scope
- <bounds so the worker doesn't creep>

## Notes
<append-only log - workers add status here>
```

### Status

Path: `agents/<name>/STATUS.md`

```markdown
---
name: worker-auth
state: active | idle | blocked | gone
updated_at: 20260413T100000Z
current_task: T-017
blockers: null
---
One-line freeform description of what I'm doing right now.
```

### Role

Path: `agents/<name>/ROLE.md`

```markdown
---
name: worker-auth
scope: "Auth handlers under crates/app/src/http/auth"
files_owned:
  - crates/app/src/http/auth/**
constraints:
  - Do not touch database/models
  - Do not edit migrations
---
```

## Rules for workers

1. One writer per file. `files_owned` is a contract. Never edit a file owned by another agent; send a message instead.
2. Atomic moves, not copies. `claim`/`release`/`close` move task files between directories; never duplicate.
3. Append-only logs. The `## Notes` section in tasks and `broadcast/` entries are append-only.
4. Heartbeat or go. If you stop working, set `state: gone` and `leave`.
5. Real newlines. When writing YAML/markdown, use actual line breaks. No `\n` escapes.
6. UTC timestamps only. Always use `date -u +%Y%m%dT%H%M%SZ`.
7. The orchestrator does not implement. If asked to code, create and assign a task.
