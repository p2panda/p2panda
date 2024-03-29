# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Highlights are marked with a pancake 🥞

## [Unreleased]

## [0.8.1]

Released on 2023-11-19: :package: `p2panda-js` and :package: `p2panda-rs`

### Fixed

- Fix de-/serialization for human-readable encodings [#529](https://github.com/p2panda/p2panda/pull/529) `rs`

## [0.8.0]

Released on 2023-10-12: :package: `p2panda-js` and :package: `p2panda-rs`

### Changed

- Remove unused methods from `EntryStore` [#521](https://github.com/p2panda/p2panda/pull/521) `rs`
- Serialize `Hash` and derived id types from bytes [#525](https://github.com/p2panda/p2panda/pull/525) `rs` `js`
- New `Reducer` trait used during document building [#527](https://github.com/p2panda/p2panda/pull/527) `rs`

### Fixed

- Fix missing bytes field validation in schema field definition [#519](https://github.com/p2panda/p2panda/pull/519) `rs`
- Make sure that pieces field in `blob_v1` is not empty [#523](https://github.com/p2panda/p2panda/pull/523) `rs`
- Remove confusing `test-utils` macros in docs [#526](https://github.com/p2panda/p2panda/pull/526) `rs`

## [0.7.1]

Released on 2023-07-27: :package: `p2panda-js` and :package: `p2panda-rs`

### Added

- Implement serde for `SchemaName` and `SchemaDescription` [#487](https://github.com/p2panda/p2panda/pull/487) `rs`
- Implement `Ord` for `SchemaId` [#492](https://github.com/p2panda/p2panda/pull/492) `rs`
- Add `Bytes` field to `OperationValue` and `PlainValue` [513](https://github.com/p2panda/p2panda/pull/513) `rs` `js`
- Introduce `blob_v1` and `blob_piece_v1` system schema [#507](https://github.com/p2panda/p2panda/pull/507) `rs`

### Changed

- Serialize `PublicKey` as bytes [#495](https://github.com/p2panda/p2panda/pull/495) `rs`
- Remove next log id requirement in `publish` validation [#502](https://github.com/p2panda/p2panda/pull/502) `rs`

### Fixed

- Handle parsing empty pinned relation lists from `PlainOperation` [#489](https://github.com/p2panda/p2panda/pull/489) `rs`

### Lion on the street

- CI: Fix coverage task and update all GH actions [#501](https://github.com/p2panda/p2panda/pull/501)

## [0.7.0]

Released on 2023-03-06: :package: `p2panda-js` and :package: `p2panda-rs`

### Changed

- Validate an operation's schema id matches the target document's in `publish` [#486](https://github.com/p2panda/p2panda/pull/486) `rs`
- Introduce `SchemaName`, `SchemaDescription` and `SchemaFields` structs [#481](https://github.com/p2panda/p2panda/pull/481) `rs`
- Add `commit` method to `Document` for applying operations incrementally [#485](https://github.com/p2panda/p2panda/pull/485) `rs` 🥞
- Introduce `api` module which publicly exports `publish` and `next_args` [#483](https://github.com/p2panda/p2panda/pull/483) `rs`
- Add schema name tests when deserializing plain operations [#480](https://github.com/p2panda/p2panda/pull/480) `rs`
- Introduce typed errors in `domain` and `validation` modules [#478](https://github.com/p2panda/p2panda/pull/478) `rs`
- Refactor storage API: Rename methods, remove `StorageProvider` and document "caching" layers [#469](https://github.com/p2panda/p2panda/pull/469) `rs` 🥞
- Remove `VerifiedOperation` [#465](https://github.com/p2panda/p2panda/pull/465) `rs`
- Better docs for `Document` [#470](https://github.com/p2panda/p2panda/pull/470) `rs`
- Remove `DocumentMeta` [#472](https://github.com/p2panda/p2panda/pull/472) `rs`
- Update dependencies for Rust and TypeScript [#476](https://github.com/p2panda/p2panda/pull/476) `rs` `js`

## [0.6.0] 🥞

Released on 2022-09-07: :package: `p2panda-js` and :package: `p2panda-rs`

### Changed

- Rename `previousOperations` field to `previous`, add examples [#459](https://github.com/p2panda/p2panda/pull/459) `js`
- Updated diagrams and doc-strings [#460](https://github.com/p2panda/p2panda/pull/460) `rs`
- Rename `Author` to `PublicKey` [#461](https://github.com/p2panda/p2panda/pull/461) `rs` `js`
- Rename `previous_operations` to `previous` [#461](https://github.com/p2panda/p2panda/pull/461) `rs` `js`

### Fixed

- Fix validation of relations pointing at system schema ids [#453](https://github.com/p2panda/p2panda/pull/453) `rs`

## [0.5.0]

Released on 2022-08-19: :package: `p2panda-js` and :package: `p2panda-rs`

### Added

- `MemoryStore` in memory implementation of storage traits [#383](https://github.com/p2panda/p2panda/pull/383) `rs`
- Helpers and conversion implementations to create schemas and operations more easily [#416](https://github.com/p2panda/p2panda/pull/416) `rs`
- Untagged operation format, schema validation, new operation and entry API [#415](https://github.com/p2panda/p2panda/pull/415) `rs`
- Serde trait implementations for `DocumentId` and all relations [#446](https://github.com/p2panda/p2panda/pull/446) `rs`
- Introduce new low-level API for `p2panda-js`, move `Session` into new repository [#447](https://github.com/p2panda/p2panda/pull/447) `js`

### Changed

- Refactor mock `Node` implementation to use `StorageProvider` traits [#383](https://github.com/p2panda/p2panda/pull/383) `rs`
- Deserialize from string and u64 for `LogId` and `SeqNum` [#401](https://github.com/p2panda/p2panda/pull/401) `rs`
- Add latest_log_id method to `LogStore` [#413](https://github.com/p2panda/p2panda/pull/413) `rs`
- Remove generic parameters from `StorageProvider` [#408](https://github.com/p2panda/p2panda/pull/408) `rs`
- Consistent `as_str` and `to_string` functions, introduce `Human` trait with `display` method for short strings [#389](https://github.com/p2panda/p2panda/pull/389) `rs`
- Update `Human` impl for `SchemaId` and `DocumentViewId` [#414](https://github.com/p2panda/p2panda/pull/414) `rs`
- Deprecate mock `Node` and `Client` structs [#437](https://github.com/p2panda/p2panda/pull/437)
- Introduce `validation` and `domain` modules to `test_utils/db` [#437](https://github.com/p2panda/p2panda/pull/437)
- Introduce new node and browser builds for JavaScript, export TypeScript definitions [#429](https://github.com/p2panda/p2panda/pull/429) `js`
- Refactored benchmarks to include schema validation [#430](https://github.com/p2panda/p2panda/pull/414) `rs`
- Replace `@apollo/client` with `graphql-request` [#441](https://github.com/p2panda/p2panda/pull/441) `js`
- Expose `from_bytes` for `EncodedOperation` and `EncodedEntry` [#445](https://github.com/p2panda/p2panda/pull/445) `rs`
- Introduce new feature flags, rename `testing` to `test-utils` [#448](https://github.com/p2panda/p2panda/pull/448) `rs`
- Replace `lazy_static` with `once_cell` [#449](https://github.com/p2panda/p2panda/pull/449) `rs`
- Build ES Module, CommonJS, NodeJS and UMD modules with rollup [#450](https://github.com/p2panda/p2panda/pull/450) `js`
- Require `DocumentStore` trait on `StorageProvider` [#456](https://github.com/p2panda/p2panda/pull/456) `rs`

### Fixed

- Set log id default to `0` [#398](https://github.com/p2panda/p2panda/pull/398) `rs`
- Fix iterator implementations for `SeqNum` and `LogId` [#404](https://github.com/p2panda/p2panda/pull/404) `rs`
- Fix system schema CDDL definitions [#393](https://github.com/p2panda/p2panda/pull/393) `rs`
- Fix GraphQL queries via Apollo [#428](https://github.com/p2panda/p2panda/pull/428) `js`

## [0.4.0]

Released on 2022-07-01: :package: `p2panda-js` and :package: `p2panda-rs`

### Added

- `Document` for sorting and reducing a graph of `Operations` [#169](https://github.com/p2panda/p2panda/pull/169) `rs` 🥞
- Derive `Ord` and `PartialOrd` for `LogId` [#201](https://github.com/p2panda/p2panda/pull/201) `rs`
- Rename `SchemaBuilder` to `CDDLBuilder` [#226](https://github.com/p2panda/p2panda/pull/226) `rs`
- `SchemaView` and `SchemaFieldView` for representing materialised system documents [#226](https://github.com/p2panda/p2panda/pull/226) `rs`
- `relation` & `relation_list` field type [#205](https://github.com/p2panda/p2panda/pull/205) `rs` `js`
- `SchemaId` enum for identifying different schema types [#221](https://github.com/p2panda/p2panda/pull/221) `rs`
- CDDL for _schema_v1_ and _schema_field_v1_, use `cddl-cat` instead of `cddl` [#248](https://github.com/p2panda/p2panda/pull/248) `rs`
- `Schema` for representing application schema [#250](https://github.com/p2panda/p2panda/pull/250) `rs`
- Performance benchmarks for entry and operation encoding/decoding [#254](https://github.com/p2panda/p2panda/pull/254) `rs`
- Move `DocumentId` from `DocmentView` into `Document` [#255](https://github.com/p2panda/p2panda/pull/255) `rs`
- Introduce `OperationId` to increase type safety around uses of `Hash` [#272](https://github.com/p2panda/p2panda/pull/272) `rs`
- `StorageProvider` and associated traits for implementing storage solutions [#274](https://github.com/p2panda/p2panda/pull/274) `rs` 🥞
- Implement `Display` trait for various structs [#281](https://github.com/p2panda/p2panda/pull/281) `rs`
- Implement document view id hash as a limited-size identifier for document views [#277](https://github.com/p2panda/p2panda/pull/277) `rs`
- Additional methods on `EntryStore` needed for replication [#310](https://github.com/p2panda/p2panda/pull/310) `rs`
- Introduce `DocumentViewHash`, implement `Hash` for `DocumentViewId` [#313](https://github.com/p2panda/p2panda/pull/313) `rs`
- Introduce `DocumentViewFields` & `DocumentViewValue` and other `Document` additions [#319](https://github.com/p2panda/p2panda/pull/319) `rs`
- Storage traits for `Operation` [#326](https://github.com/p2panda/p2panda/pull/326) `rs`
- Implement schema hash id as a unique identifier for schemas `rs` [#282](https://github.com/p2panda/p2panda/pull/282) `rs`
- `Graph` method for selecting sub-section of graph [#335](https://github.com/p2panda/p2panda/pull/335) `rs`
- Storage traits for documents [#343](https://github.com/p2panda/p2panda/pull/343) `rs`
- Materialise a document at a specific document view [#337](https://github.com/p2panda/p2panda/pull/337) `rs`
- Static definitions of system schemas and other updates for schema provider in aquadoggo [#365](https://github.com/p2panda/p2panda/pull/365) `rs`

### Changed

- `Instance` renamed to `DocumentView` [#169](https://github.com/p2panda/p2panda/pull/169) `rs`
- Fix letter casing in operations [#230](https://github.com/p2panda/p2panda/pull/230) `rs` `js`
- Fixes and refactorings around schema [#233](https://github.com/p2panda/p2panda/pull/233) `rs`
- Split `Relation` into pinned and unpinned type [#235](https://github.com/p2panda/p2panda/pull/235) `rs`
- Separate `cddl` from `schema` more clearly [#239](https://github.com/p2panda/p2panda/pull/239) `rs`
- Turn schema field in operations into a pinned relation [#256](https://github.com/p2panda/p2panda/pull/256) `rs`
- Implement `OperationValue` variants for all relation types [#260](https://github.com/p2panda/p2panda/pull/260) `rs` `js`
- Support all `Relation` flavours in `cddl` module [#259](https://github.com/p2panda/p2panda/pull/259) `rs`
- Impl `IntoIter` trait for `PinnedRelation`, `RelationList` and `DocumentViewId` [#266](https://github.com/p2panda/p2panda/pull/266) `rs`
- Improve error reporting when adding operation fields [#262](https://github.com/p2panda/p2panda/issues/262) `rs` `js`
- Update mock node API [#286](https://github.com/p2panda/p2panda/issues/286) `rs`
- Refactored graph module to be generic over graph node keys and other graph improvements [#289](https://github.com/p2panda/p2panda/issues/289) `rs`
- Require sorted serialisation of document view ids [#284](https://github.com/p2panda/p2panda/pull/284) `rs`
- Introduce new application schema id format [#292](https://github.com/p2panda/p2panda/pull/292) `rs`
- Update spelling of system schema ids [#294](https://github.com/p2panda/p2panda/pull/294) `rs`
- Update `Schema` implementation to make use of new `SchemaId` [#296](https://github.com/p2panda/p2panda/pull/296) `rs`
- Require schema field definitions to specify a specific schema [#269](https://github.com/p2panda/p2panda/pull/269) `rs` 🥞
- Methods for getting string representations of `OperationValue` field type and `OperationAction` [#303](https://github.com/p2panda/p2panda/pull/303) `rs`
- Additional constructor method for `OperationWithMeta` [#322](https://github.com/p2panda/p2panda/pull/322) `rs`
- Minor method renaming in `EntryStore` [#323](https://github.com/p2panda/p2panda/pull/323) `rs`
- Require storage provider errors to be thread-safe [#340](https://github.com/p2panda/p2panda/pull/340)
- Make `previous_operations` a `DocumentViewId` [#342](https://github.com/p2panda/p2panda/pull/342) `rs`
- Restructure / refactor `test_utils` and place behind `testing` flag [#344](https://github.com/p2panda/p2panda/pull/344) `rs`
- Update `openmls` crate to `v0.4.1` [#336](https://github.com/p2panda/p2panda/pull/336) `rs`
- Replace `OperationWithMeta` with `VerifiedOperation` [#353](https://github.com/p2panda/p2panda/pull/353) `rs`
- Remove test-data generator from `test_utils` [#373](https://github.com/p2panda/p2panda/pull/373) `rs`
- Implement `OperationStore` on test provider `SimplestStorageProvider` [#361](https://github.com/p2panda/p2panda/pull/361) `rs`
- Improve validation in `EntrySigned` constructor [#367](https://github.com/p2panda/p2panda/pull/367) `rs`
- `Session` interface using GraphQL [#364](https://github.com/p2panda/p2panda/pull/377) `js`
- Updated dependencies, remove `automock` crate [#379](https://github.com/p2panda/p2panda/pull/379) `rs`

### Fixed

- Fix determination of field types in p2panda-js [#202](https://github.com/p2panda/p2panda/pull/202) `js`
- Fix equality of document view ids by sorting before comparison [#284](https://github.com/p2panda/p2panda/pull/284) `js`
- Pin all versions in `Cargo.toml` to avoid unexpected crate updates [#299](https://github.com/p2panda/p2panda/pull/299) `rs`
- Fix document test needing `testing` feature to be activated [#350](https://github.com/p2panda/p2panda/pull/350) `rs`

### Everything burrito

- Easier to read CDDL schema error strings [#207](https://github.com/p2panda/p2panda/pull/207) `rs`
- Force cache cleanup to fix code coverage report [#231](https://github.com/p2panda/p2panda/pull/231)
- Split up overly long `operation.rs` file [#232](https://github.com/p2panda/p2panda/pull/232) `rs`
- Extend test coverage for `OperationFields` [#236](https://github.com/p2panda/p2panda/pull/236) `rs`
- Further develop our best practices for writing documentation [#240](https://github.com/p2panda/p2panda/pull/240) `rs`
- Test `debug` macro calls in Github CI [#288](https://github.com/p2panda/p2panda/pull/288) `rs`
- Move private module doc strings into public places [#339](https://github.com/p2panda/p2panda/pull/339) `rs`
- Add `mockall` crate and create mocks for `EntryStore` and `LogStore` [#314](https://github.com/p2panda/p2panda/pull/314) `rs`
- Generate documentation with TypeDoc for `p2panda-js` [#359](https://github.com/p2panda/p2panda/pull/359) `js`

## [0.3.0]

Released on 2022-02-02: :package: `p2panda-js` and 2022-06-11: :package: `p2panda-rs`

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
- Generate CBOR encoded test data [300](https://github.com/p2panda/p2panda/pull/300) `rs`

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

[unreleased]: https://github.com/p2panda/p2panda/compare/v0.8.1...HEAD
[0.8.1]: https://github.com/p2panda/p2panda/releases/tag/v0.8.0
[0.8.0]: https://github.com/p2panda/p2panda/releases/tag/v0.8.0
[0.7.1]: https://github.com/p2panda/p2panda/releases/tag/v0.7.1
[0.7.0]: https://github.com/p2panda/p2panda/releases/tag/v0.7.0
[0.6.0]: https://github.com/p2panda/p2panda/releases/tag/v0.6.0
[0.5.0]: https://github.com/p2panda/p2panda/releases/tag/v0.5.0
[0.4.0]: https://github.com/p2panda/p2panda/releases/tag/v0.4.0
[0.3.0]: https://github.com/p2panda/p2panda/releases/tag/v0.3.0
[0.2.1]: https://github.com/p2panda/p2panda/releases/tag/v0.2.1
[0.2.0]: https://github.com/p2panda/p2panda/releases/tag/v0.2.0
[0.1.0]: https://github.com/p2panda/p2panda/releases/tag/v0.1.0
