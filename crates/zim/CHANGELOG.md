# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.4](https://github.com/krondor-corp/zim/compare/zim-v0.1.3...zim-v0.1.4) - 2026-10-28

### Added

- *(mount)* zim mount start re-mounts a stopped registration (KRO-232) ([#50](https://github.com/krondor-corp/zim/pull/50))
- *(daemon)* vault create seals to every device on the paired account
- *(core)* genesis shares carry their own reach

### Fixed

- *(mount)* refuse re-adding a registered mountpoint (KRO-233) ([#52](https://github.com/krondor-corp/zim/pull/52))
- *(mount)* make mount stop idempotent and clear ghost volumes ([#49](https://github.com/krondor-corp/zim/pull/49))
- *(daemon)* make service stop stick under launchd; add restart (KRO-208) ([#48](https://github.com/krondor-corp/zim/pull/48))
- *(mount)* commit staged FUSE writes before mount stop/remove ([#46](https://github.com/krondor-corp/zim/pull/46))
- *(cli)* zim mount set accepts bare --auto/--read-only and --ro alias ([#47](https://github.com/krondor-corp/zim/pull/47))
- *(log)* head links round-trip with the manifest's CBOR codec (KRO-258) ([#45](https://github.com/krondor-corp/zim/pull/45))
- *(fuse)* enable macfuse-4-compat only on macOS ([#44](https://github.com/krondor-corp/zim/pull/44))
- *(fuse)* coalesce an editor's save burst into one debounced commit ([#42](https://github.com/krondor-corp/zim/pull/42))
- refuse sealing a vault to the hub; hide the hub from peers list (KRO-229) ([#41](https://github.com/krondor-corp/zim/pull/41))

## [0.1.3](https://github.com/krondor-corp/zim/compare/zim-v0.1.2...zim-v0.1.3) - 2026-10-06

### Other

- *(core)* shares are sealed at grant; recovery lives in share.rs with ShareError
- *(core)* hoist imports; explicit share-height invariant; SharePending error

## [0.1.2](https://github.com/krondor-corp/zim/compare/zim-v0.1.1...zim-v0.1.2) - 2026-07-28

### Other

- Pack-aligned redesign: one-product site + pack UI for the hub app ([#24](https://github.com/krondor-corp/zim/pull/24))

## [0.1.1](https://github.com/krondor-corp/zim/compare/zim-v0.1.0...zim-v0.1.1) - 2026-07-27

### Other

- Deep e2e validation + the four sync bugs it flushed out ([#18](https://github.com/krondor-corp/zim/pull/18))
