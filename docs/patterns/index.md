# Patterns

Cross-cutting rules and contracts used across Zim's subsystems. Subsystem
implementation belongs in [`architecture/`](../architecture/index.md).

| Document | Purpose |
|---|---|
| [Conventions](conventions.md) | Error handling, async, serialization, tests, and module organization |
| [CLI](cli.md) | Op pattern and formatting boundary |
| [HTTP API](http-api.md) | Daemon and hub HTTP contract |
| [Success Criteria](success-criteria.md) | Required build, test, lint, and format checks |

Domain behavior and security guarantees belong in
[`product/`](../product/index.md). Local workflows and diagnosis belong in
[`dx/`](../dx/index.md).
