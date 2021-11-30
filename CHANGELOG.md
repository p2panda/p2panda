# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Highlights are marked with a pancake 🥞

## [Unreleased]

### Added

- `SecureGroup` for key negotiation in groups and message protection [#130](https://github.com/p2panda/p2panda/pull/130) `rs` 🥞
- `SchemaBuilder` and `Schema` structs for working with CDDL definitions [#78](https://github.com/p2panda/p2panda/pull/78) `rs`
- `test_utils` module containing `rstest` fixtures, mock `Node` and `Client` structs, test data helper for `p2panda-js` [#116](https://github.com/p2panda/p2panda/pull/116) `rs`

### Changed

- Update `ed25519` crate to `1.3.0` and deprecated `Signature` API [#137](https://github.com/p2panda/p2panda/pull/137) `rs`

## [0.2.1]

Released on 2021-10-26: :package: `p2panda-js` :package: `p2panda-rs`

### Fixed

- Use deprecated linter name for now as it breaks some building pipelines [#121](https://github.com/p2panda/p2panda/pull/121) `rs`
- Fix access to optional parameter in `Session.query` logging call [#119](https://github.com/p2panda/p2panda/issues/119) `js`

### Campfires and boiling pots to sit around

- Add pull request template to not forget updating this changelog [#122](https://github.com/p2panda/p2panda/pull/122)

## [0.2.0]

Released on 2021-10-25: :package: `p2panda-js` :package: `p2panda-rs`

### Added

- `Relation` message values [#33](https://github.com/p2panda/p2panda/pull/33) `rs`
- Atomic data types like `Message`, `Entry`, `LogId`, `SeqNum`, etc. [#23](https://github.com/p2panda/p2panda/pull/23) `rs` 🥞
- `sqlx` [Type](https://docs.rs/sqlx/0.5.1/sqlx/trait.Type.html) traits to atomic structs, introduce `db-sqlx` feature flag [#43](https://github.com/p2panda/p2panda/pull/43) `rs`
- `serde` serialization and deserialization for all atomic structs [#42](https://github.com/p2panda/p2panda/pull/42) `rs`
- Implement method for iterating over MessageFields [#68](https://github.com/p2panda/p2panda/pull/68) `rs`
- TypeScript API that wraps wasm bindings, introduce OpenRPC specification [#67](https://github.com/p2panda/p2panda/pull/67) `js` 🥞
- Methods to update and delete instances [#114](https://github.com/p2panda/p2panda/pull/114) `js` 🥞

### Changed

- Change all result types to return `std::Result` and custom p2panda-rs errors [#41](https://github.com/p2panda/p2panda/pull/41) `rs`
- Move WebAssembly related code into own `wasm` module [#49](https://github.com/p2panda/p2panda/pull/49) `rs`
- Own module for encoding, decoding and signing entries [#62](https://github.com/p2panda/p2panda/pull/62) `rs`
- General module restructure [#69](https://github.com/p2panda/p2panda/pull/69) `rs`
- Add support for different message values in WebAssembly [#71](https://github.com/p2panda/p2panda/pull/71) `rs`
- Extend `jserr` macro to support custom error messages [#75](https://github.com/p2panda/p2panda/pull/75) `rs`
- Use published `bamboo-rs-core` crate [#94](https://github.com/p2panda/p2panda/pull/94) `rs`
- `p2panda-js` directory restructure [#102](https://github.com/p2panda/p2panda/pull/102) `js`
- Use Jest as test framework [#104](https://github.com/p2panda/p2panda/pull/104) `js`
- Clean up OpenRPC generate script [#109](https://github.com/p2panda/p2panda/pull/109) `tests`
- Refactor and simplify WebAssembly build pipeline [#105](https://github.com/p2panda/p2panda/pull/105) `rs` `js`
- Revisit singleton logic of WebAssembly import [#110](https://github.com/p2panda/p2panda/pull/110) `js`
- Move WebAssembly methods of KeyPair into dedicated module [#111](https://github.com/p2panda/p2panda/pull/111) `rs`

### Fixed

- Fix wrong offset of skiplinks [#46](https://github.com/p2panda/p2panda/pull/46) `rs`
- Assure deterministic hashing by ordering of message keys [84a583](https://github.com/p2panda/p2panda/commit/84a583eb58614e8c5ae76c80f2f04ee96db98713) `rs`
- Remove `BigInt` to support WebKit [#66](https://github.com/p2panda/p2panda/pull/66) `rs`
- Properly import entry tests module [#81](https://github.com/p2panda/p2panda/pull/81) `rs`
- Correct error in `panda_queryEntries` OpenRPC specification result [#108](https://github.com/p2panda/p2panda/pull/108) `tests`

### Campfires and boiling pots to sit around

- Write examples in Rust documentation [#59](https://github.com/p2panda/p2panda/pull/59) `rs` 🥞
- Add SPDX license headers to all files [#86](https://github.com/p2panda/p2panda/pull/86) `rs` `js`
- Run tests with node version matrix [#98](https://github.com/p2panda/p2panda/pull/98) `tests`

## [0.1.0]

Released on 2021-01-18: :package: `p2panda-js` and 2021-01-28: :package: `p2panda-rs`

### Added

- JavaScript library export with WebAssembly running in browsers and NodeJS. [#21](https://github.com/p2panda/p2panda/pull/21) `js`
- Ed25519 key pair generation. [#4](https://github.com/p2panda/p2panda/pull/4) `rs`

[unreleased]: https://github.com/p2panda/p2panda/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/p2panda/p2panda/releases/tag/v0.2.1
[0.2.0]: https://github.com/p2panda/p2panda/releases/tag/v0.2.0
[0.1.0]: https://github.com/p2panda/p2panda/releases/tag/v0.1.0
