---
from: orch
to: thing4
ts: 20260524T164446Z
kind: task-assign
ref: T-004
---
## URGENT — rename branch `master` → `main`.

Current branch is `master`. Rename now. No remote yet so this is local-only:

```bash
git branch -m master main
```

Then update anything that references `master`:
- `.github/workflows/*.yml` — any `branches: [master]` triggers, `on: push: branches: [master]`, etc.
- `Makefile`, `bin/*` — any `git ... master` references.
- `docs/`, `wiki/`, `README.md`, `CLAUDE.md` — text references.
- `.coord/**` notes / status — text references (low priority, leave if effort).

Sweep with: `grep -rn 'master' --include='*.yml' --include='*.md' --include='Makefile' --include='*.sh' .` then fix each.

Verify branch is renamed: `git branch --show-current` returns `main`.

Commit the references-update separately from the rename: "rename default branch master → main" as a clean small commit.

Heartbeat with the commit hash + any references you found.

## Why this is going out blunt

User flagged "master" naming as unacceptable. Not a controversial call — modern convention is `main`. Just fix it.
