# Documentation Guidelines

How Zim documentation is organized and maintained. Read this before adding a
document.

## Two Audiences, Two Homes

| | Where | Audience | Voice |
|---|---|---|---|
| Contributor docs | `docs/` | People and agents working on Zim | Precise, technical, links to code |
| End-user site | `web/` | People installing and using Zim | Plain language, task-oriented, operational |

Do not mix them. Architecture, crate paths, and internal types belong in
`docs/`. Installation steps and user workflows belong in `web/`. See
[Web Guidelines](web.md) for the public site rules.

## Documentation Modules

| Module | Purpose |
|---|---|
| [`product/`](../product/index.md) | What Zim is and does: capabilities, user-visible concepts, guarantees, and roadmap |
| [`architecture/`](../architecture/index.md) | How Zim is built: subsystem boundaries, data relationships, flows, and invariants |
| [`ui/`](../ui/index.md) | How the browser interface and WASM boundary are built |
| [`patterns/`](../patterns/index.md) | Cross-cutting implementation rules and contracts used by multiple subsystems |
| [`dx/`](../dx/index.md) | Working on Zim locally: setup, development, debugging, testing, and contribution |
| [`devops/`](../devops/index.md) | Releasing, deploying, and operating Zim |
| [`research/`](../research/) | Design investigations that are not authoritative descriptions of shipped behavior |
| `_guidelines/` | How documentation itself is organized |

## Product, Architecture, And Patterns

These three modules answer different questions:

- **Product:** What capability does Zim provide, what does it mean to a user,
  and what guarantees or limitations are externally meaningful? Product pages
  do not contain Rust struct dumps, source paths, storage layouts, or protocol
  implementation.
- **Architecture:** How does a subsystem implement those capabilities?
  Architecture pages describe durable boundaries, relationships, flows, and
  invariants. Organize substantial subjects as a directory with an `index.md`,
  such as `architecture/vaults/`.
- **Patterns:** Which rules apply repeatedly across subsystems? Error handling,
  CLI operation boundaries, HTTP contracts, testing conventions, and lifecycle
  patterns belong here. A subsystem is not a pattern.

Exact private fields, constructor signatures, serde annotations, and error
variants belong beside the code in Rustdoc. Architecture docs should explain
why those types relate, not duplicate declarations that drift with refactors.

When unsure, ask whether the reader is trying to understand a capability,
understand its implementation, or apply a reusable engineering rule.

## Conventions

- **Document the non-obvious.** Capture invariants, decisions, boundaries,
  failure modes, and reasons. Avoid duplicating file listings that can be
  discovered directly, except where a short map is needed for orientation.
- **Use lowercase filenames.** A module index is always `index.md`, not
  `README.md`.
- **Use module indexes.** Each main module has an `index.md` that orients the
  reader and links to focused topic pages.
- **Group architecture by subsystem.** A substantial subsystem gets
  `architecture/<subject>/index.md` plus focused pages; do not accumulate one
  repository-wide data-model dump.
- **Match shipped reality.** Do not describe roadmap items as implemented.
  Research and proposals must be clearly labeled.
- **Keep concerns separated.** User workflows stay out of contributor docs;
  implementation details stay out of the public site.
- **Link instead of copying.** Link to source symbols and canonical docs rather
  than reproducing code or maintaining duplicate explanations.
- **Update navigation.** Add new contributor docs to the relevant module index
  and `docs/index.md`; add new user pages to `web/_data/nav.yml`.
- **Update skills with behavior.** If commands, review rules, Linear workflow,
  or release workflows change, update the corresponding `.claude/skills/` file.
