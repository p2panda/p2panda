# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Highlights are marked with a pancake 🥞

## [Unreleased]

## Added

- `Document` for sorting and reducing a graph of `Operations` [#169](https://github.com/p2panda/p2panda/pull/169) `rs` 🥞
- Derive `Ord` and `PartialOrd` for `LogId` [#201](https://github.com/p2panda/p2panda/pull/201)
- `SchemaHash` enum for identifying different schema types [#221](https://github.com/p2panda/p2panda/pull/221) `rs`

## Changed

- `Instance` renamed to `DocumentView` [#169](https://github.com/p2panda/p2panda/pull/169) `rs`

## Fixed

- Fix determination of field types in p2panda-js [#202](https://github.com/p2panda/p2panda/pull/202)

## Everything burrito

- Easier to read CDDL schema error strings [#207](https://github.com/p2panda/p2panda/pull/207)

## [0.3.0]

Released on 2022-02-02: :package: `p2panda-js`

Not released yet, due to unpublished dependencies: :package: `p2panda-rs`

### Added

- `SecureGroup` for key negotiation in groups and message protection [#130](https://github.com/p2panda/p2panda/pull/130) `rs` 🥞
- `SchemaBuilder` and `Schema` structs for working with CDDL definitions [#78](https://github.com/p2panda/p2panda/pull/78) `rs`
- `test_utils` module containing `rstest` fixtures, mock `Node` and `Client` structs, test data helper for `p2panda-js` [#116](https://github.com/p2panda/p2panda/pull/116) `rs`
- Reconciliation logic /w DAG for materialisation module [#129](https://github.com/p2panda/p2panda/pull/129) `rs`
- `Instance` which encapsulates the materialised view of a reduced collection of `Operations` [#161](https://github.com/p2panda/p2panda/pull/161) `rs`
- Retrieve unsigned bytes to verify `Entry` signatures manually [#197](https://github.com/p2panda/p2panda/pull/197/files) `rs`

### Changed

- Adopt [Blake3](https://github.com/BLAKE3-team/BLAKE3) hashes, which bring us better performance and shorter identifiers [#139](https://github.com/p2panda/p2panda/pull/139) `rs` 🥞
- Update `ed25519` crate to `1.3.0` and deprecated `Signature` API [#137](https://github.com/p2panda/p2panda/pull/137) `rs`
- Use new `Operation` naming which replaces `Message` [#156](https://github.com/p2panda/p2panda/pull/156) _BREAKING_ `rs` `js`
- Remove distinction of system and application log ids [#154](https://github.com/p2panda/p2panda/pull/154) `rs`
- Update JavaScript dependencies, remove deprecated eslint-loader [#155](https://github.com/p2panda/p2panda/pull/155) `js`
- Split utils modules in `test_utils` into utils.rs and constants.rs [#157](https://github.com/p2panda/p2panda/pull/157) `rs`
- Use traits for validation methods in `Schema` [#160](https://github.com/p2panda/p2panda/pull/160) `rs`
- Add `previous_operations` field in `Operation` [#163](https://github.com/p2panda/p2panda/pull/163) _BREAKING_ `rs` `js`
- Introduce `OperationWithMeta` struct [#163](https://github.com/p2panda/p2panda/pull/163) `rs`
- Update API and mocks to reflect yasmf hash and document flow changes [#165](https://github.com/p2panda/p2panda/pull/165) _BREAKING_ `rs` `js`
- Change to new `rustdoc::missing_doc_code_examples` linter name [#168](https://github.com/p2panda/p2panda/pull/168) `rs`
- Update Rust dependencies [#171](https://github.com/p2panda/p2panda/pull/171) `rs`
- Convert JavaScript configuration files to TypeScript or JSON [#172](https://github.com/p2panda/p2panda/pull/172) `js`
- Implement `Hash`, `Eq` and `PartialEq` traits for several core data types [#178](https://github.com/p2panda/p2panda/pull/178) `rs`
- Use `ciborium` for cbor de/serialization [#180](https://github.com/p2panda/p2panda/pull/180) `rs`
- Break `wasm` module down into sub-files, add wasm target tests [#184](https://github.com/p2panda/p2panda/pull/184) `rs`
- Changes to `mocks` module in `test_utils` [#181](https://github.com/p2panda/p2panda/pull/181) `rs`
- Implement logging for mock node in `test_utils` [#192](https://github.com/p2panda/p2panda/pull/192) `rs`
- Support `u64` and `i64` integers, remove `sqlx` [#177](https://github.com/p2panda/p2panda/pull/177) `rs` `js`

### Campfires and boiling pots to sit around

- Update `test_utils` documentation [#152](https://github.com/p2panda/p2panda/pull/152) `rs`
- Make clippy happy, add CI for linter checks [#153](https://github.com/p2panda/p2panda/pull/153) `rs`
- Clean up documentation and update new terminology [#170](https://github.com/p2panda/p2panda/pull/170) `rs` `js`
- Improve CI, make it faster, add code coverage report [#173](https://github.com/p2panda/p2panda/pull/173) `rs` `js`
- Update Codecov GH action [#176](https://github.com/p2panda/p2panda/pull/176) `rs`
- Add JavaScript coverage reporting [#194](https://github.com/p2panda/p2panda/pull/194) `js`

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
- `serde` serialisation and deserialisation for all atomic structs [#42](https://github.com/p2panda/p2panda/pull/42) `rs`
- Implement method for iterating over MessageFields [#68](https://github.com/p2panda/p2panda/pull/68) `rs`
- TypeScript API that wraps wasm bindings, introduce OpenRPC specification [#67](https://github.com/p2panda/p2panda/pull/67) `js` 🥞
- Methods to update and delete documents [#114](https://github.com/p2panda/p2panda/pull/114) `js` 🥞

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

[unreleased]: https://github.com/p2panda/p2panda/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/p2panda/p2panda/releases/tag/v0.3.0
[0.2.1]: https://github.com/p2panda/p2panda/releases/tag/v0.2.1
[0.2.0]: https://github.com/p2panda/p2panda/releases/tag/v0.2.0
[0.1.0]: https://github.com/p2panda/p2panda/releases/tag/v0.1.0
