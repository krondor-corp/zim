---
from: orch
ts: 20260524T015900Z
kind: policy-clarification
audience: all
ref: broadcast/20260524T015636Z-pack-design-language.md
---
# Clarification: pack is AESTHETIC only

The pack-design-language broadcast was overscoped. Walking it back:

- **Pack is an aesthetic reference** — folder layout, docs structure, wiki shape, web UI feel, naming conventions.
- **Pack is NOT a binding source of architectural patterns or libraries.** `runtime::Service`, `struct-patch`, Apalis, MCP, handler-per-file, scoped events — these are *interesting patterns to look at*, not requirements.

The original broadcast at `20260524T015636Z-pack-design-language.md` has been rewritten to reflect this. Key consequences:

1. **Crate split unchanged.** The 6-crate target in `docs/CRATES.md` stands. No collapse to pack's 3-crate shape.
2. **T-010 downscoped** — just an aesthetic sanity check on `docs/CRATES.md` naming, no re-architecture.
3. **T-007 references demoted** — pack and jig are both inspiration, neither is the "right answer" to adopt.
4. **T-002 patterns demoted** — Apalis, MCP, runtime::Service are optional. The zim-hub designer picks what fits.
5. **Datastar still stays** (not HTMX). **SQLite still stays** (not Postgres). **No Yjs collab** (non-collaborative editor target).
6. **docs/ reshape (T-011) still on** — that's aesthetic, fits the new policy.
7. **wiki/ phase 1 (thing4) still on** — that's aesthetic, unchanged.

Carry on. The functional inspiration list in the rewritten broadcast is there if you want to peek; nothing on it is mandatory.
