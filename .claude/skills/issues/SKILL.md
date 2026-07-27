---
description: Discover and manage Zim work in Linear. Use to explore tasks before spawning workers or to track project progress.
allowed-tools:
  - Bash(jig:*)
  - Read
  - Glob
  - Grep
---

Discover and manage active work through Linear using `jig issues`. Linear is
the sole execution tracker for status, priority, ownership, and dependencies.
`docs/product/roadmap/` preserves longer-lived product context but is not a
task tracker.

## Discovery

```bash
jig issues                              # Active issues for this repository
jig issues --priority high
jig issues --priority urgent
jig issues --unblocked --status planned
jig issues --blocked
jig issues --auto                       # Spawn-ready work
jig issues <id>                         # Full issue detail
```

Before starting work, read the full Linear issue, its blockers, and any linked
roadmap page. Treat Linear's current scope and status as authoritative.

## Actions

```bash
jig issues create "Issue title"
jig issues create -p high -l auto -b "Markdown description" "Issue title"
jig issues status <id> --status in-progress
jig issues status <id> --status blocked
jig issues complete <id>
```

Use clear acceptance criteria and link the relevant
`docs/product/roadmap/<topic>.md` page when an issue implements a roadmap
direction. Do not create repository-local task files.
