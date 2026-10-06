# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1](https://github.com/krondor-corp/zim/compare/zim-crypto-v0.1.0...zim-crypto-v0.1.1) - 2026-10-06

### Added

- *(core)* ratchet the root — shares seal ratchet state, saves stop re-minting
- *(core)* per-entry skip ratchets, stable entity identity, previous links

### Fixed

- *(zim-crypto)* drop unused rand 0.9 — crate was not self-sufficiently wasm-clean

### Other

- *(core)* entity identity derives from the ratchet; ops carry only the ratchet
- *(zim-crypto)* pin skip_ratchet =0.3.0 for the per-entry key schedule
