# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Change all result types to return `std::Result` and custom p2panda-rs errors [#41](https://github.com/p2panda/p2panda/pull/41) `rs`

### Added

- Atomic data types like `Message`, `Entry`, `LogId`, `SeqNum`, etc. [#23](https://github.com/p2panda/p2panda/pull/23) `rs`
- `sqlx` [Type](https://docs.rs/sqlx/0.5.1/sqlx/trait.Type.html) traits to atomic structs, introduce `db-sqlx` feature flag [#43](https://github.com/p2panda/p2panda/pull/43) `rs`
- `serde` serialization and deserialization for all atomic structs [#42](https://github.com/p2panda/p2panda/pull/42) `rs`

## [0.1.0]

Released: :package: 2021-01-18 `p2panda-js` - :package: 2021-01-28 `p2panda-rs`

### Added

- JavaScript library export with WebAssembly running in browsers and NodeJS. [#21](https://github.com/p2panda/p2panda/pull/21) `js`
- Ed25519 key pair generation. [#4](https://github.com/p2panda/p2panda/pull/4) `rs`

[Unreleased]: https://github.com/p2panda/p2panda/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/p2panda/p2panda/releases/tag/v0.1.0
