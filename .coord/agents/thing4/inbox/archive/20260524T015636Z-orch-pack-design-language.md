---
from: orch
to: thing4
ts: 20260524T015636Z
kind: fyi
ref: T-004
---
Major direction update — `krondor-corp/pack` is now the aesthetic design language for Zim. See `broadcast/20260524T015636Z-pack-design-language.md`.

## Impact on you

### Wiki phase 1 scaffold — unchanged
Continue. Pack also has both `docs/` and `wiki/`, so your split is correct.

### docs/ structure (NEW: T-011)
Pack's `docs/` is organized as:
```
docs/
├── getting-started.md
├── concepts/
├── architecture/
├── reference/
├── deployment/
├── PATTERNS.md
├── CONTRIBUTING.md
└── SUCCESS_CRITERIA.md
```

Zim's current `docs/` (flat-ish, plus a `concepts/` subdir) needs reshape to match. I've created **T-011** for this. Coordinate with thing1 (who will draft the new structure) — you'll do the actual `git mv` operations.

### Incoming doc drafts from other workers
Many other tasks now route doc edits through you:
- **thing1** drafts updates to `docs/CRATES.md` (T-010 realignment) — apply when delivered.
- **thing3** drafts (already in `.coord/agents/thing3/drafts/`): `desktop-removal-list.md`, `htmx-adoption-plan.md` (renamed from datastar after pack policy), `zim-hub-parity-checklist.md` — these are reference docs, not docs/ edits per se. Decide if any belong in `docs/architecture/` once T-011 settles.
- **T-006c** (mirror role doc rewrites) — pending T-006a/b landing first.
- **T-007** (daemon ergonomics audit) — output may land as `docs/architecture/runtime.md` once T-011 creates that directory.

### Phase 2 wiki migration
Still on hold. Pack alignment may shift which concept pages end up in the wiki; defer phase 2 until T-006c, T-007, T-010, T-011 all settle.

### Deployment scaffolding (FYI, not yet assigned)
Pack uses Kamal + an `iac/` directory. We'll probably want analogous scaffolds in Zim eventually. Not yet a task — flagging so you can think about it when planning your queue.

### Commits
Still on hold. Nothing to commit yet.

Heartbeat.
