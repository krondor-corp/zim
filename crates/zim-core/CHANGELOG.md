# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/krondor-corp/zim/compare/zim-core-v0.1.1...zim-core-v0.2.0) - 2026-10-06

### Added

- *(core)* rename-aware merge — an edit follows a concurrent rename
- *(core)* ratchet the root — shares seal ratchet state, saves stop re-minting
- *(core)* per-entry skip ratchets, stable entity identity, previous links

### Fixed

- *(core)* drop the dead same-hash guard on overwrite unpin
- *(core)* rm unpins removed content; a version pins exactly what it needs
- *(core)* a version pins only what it references; old ops logs no longer accumulate
- *(core)* an unsaved rewrite keeps its ratchet; one key per saved revision
- *(ci)* green on current stable clippy (1.99) — two new lint classes

### Other

- *(core)* put_metadata -> put_dir; it writes a dir body, not metadata
- *(core)* ContentStore reads by (link, key); no Entry-typed getters
- *(core)* previous = prior SAVED revision; the root has no Entry
- *(core)* shares are sealed at grant; recovery lives in share.rs with ShareError
- *(core)* hoist imports; explicit share-height invariant; SharePending error
- *(core)* Entry::file / Entry::dir take every field; no placeholder constructors
- *(core)* AddFile.previous is a plain Link; allow the enum-size lint on OpKind
- *(core)* AddFile.previous is the same null-link sentinel, boxed
- *(core)* Entry.previous is a null-link sentinel, like Manifest.previous
- *(core)* Entry carries no key and nothing optional for identity
- *(core)* a conflict sidecar is a fork of the losing entity
- *(core)* entity identity derives from the ratchet; ops carry only the ratchet

## [0.1.1](https://github.com/krondor-corp/zim/compare/zim-core-v0.1.0...zim-core-v0.1.1) - 2026-07-27

### Other

- Fix #17: fast-forward adoption + lineage-true merge ancestors ([#19](https://github.com/krondor-corp/zim/pull/19))
