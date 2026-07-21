# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Highlights are marked with a pancake 🥞

## [Unreleased]

### Added

- node: Encrypted spaces and groups management integration into high-level API [#1202](https://github.com/p2panda/p2panda/pull/1202)
- store: Introduce `ProcessorStore` for storing event metadata from p2panda pipeline [#1262](https://github.com/p2panda/p2panda/pull/1262)
- node: Implement `Orderer` `Processor` for pipeline [#1262](https://github.com/p2panda/p2panda/pull/1262)
- node: Associate key bundle, groups and spaces logs with log topic [#1264](https://github.com/p2panda/p2panda/pull/1264)
- node: Handle spaces events when processing pipeline event [#1276](https://github.com/p2panda/p2panda/pull/1276)
- node: Task for repairing spaces [#1277](https://github.com/p2panda/p2panda/pull/1277)
- node: Membership change validation in Space API [#1292](https://github.com/p2panda/p2panda/pull/1292)
- spaces: Validate write authority when processing application messages [#1295](https://github.com/p2panda/p2panda/pull/1295)
- spaces: Introduce local stream import [#1296](https://github.com/p2panda/p2panda/pull/1296)
- spaces: Compute and return events from local methods [#1290](https://github.com/p2panda/p2panda/pull/1290)
- node: Forward events resulting from local actions to app layer [#1290](https://github.com/p2panda/p2panda/pull/1290)
- node: Cancel processed stream tasks on drop of stream handles [#1300](https://github.com/p2panda/p2panda/pull/1300)
- spaces: Support partial merge of auth state into a space [#1299](https://github.com/p2panda/p2panda/pull/1299)
- node: Allow graceful closure of sync sessions [#1307](https://github.com/p2panda/p2panda/pull/1307)

### Changed

- Use `Signer` trait instead of `SigningKey` in `p2panda-core` [#1202](https://github.com/p2panda/p2panda/pull/1202)
- spaces: Remove auth resolver generic parameter [#1298](https://github.com/p2panda/p2panda/pull/1298)
- spaces: Only emit membership change events if local user is space member [#1304](https://github.com/p2panda/p2panda/pull/1304)

### Fixed

- spaces: Deterministic deserialization of `SpacesArgs` [#1264](https://github.com/p2panda/p2panda/pull/1264)
- node: Unblock task tracker by introducing "pass-through" events coming from orderer [#1267](https://github.com/p2panda/p2panda/pull/1267)
- node: Allow event processing to handle out-of-order buffering by separating i/o streams and preserve input ordering [#1271](https://github.com/p2panda/)
- encryption: Do not require pre-key bundle when initialising TwoParty state as a recipient [#1297](https://github.com/p2panda/p2panda/pull/1297)
- encryption: Update hpke-rs to v0.7.0 [#1309](https://github.com/p2panda/p2panda/pull/1309)

## [0.7.0] - 07/07/2026

### Added

- auth: More methods (`traverse_members`, `groups`) to traverse and filter graph [#1242](https://github.com/p2panda/p2panda/pull/1242)
- auth: Allow ordinary members to remove themselves from group [#1234](https://github.com/p2panda/p2panda/pull/1234)
- store: Introduce SQLite implementations of `KeySecretsStore` and `KeyRegistryStore` [#1230](https://github.com/p2panda/p2panda/pull/1230)
- store: SQLite implementations of spaces stores [#1241](https://github.com/p2panda/p2panda/pull/1241)
- core: Provenance trait to get author & verify [#1254](https://github.com/p2panda/p2panda/pull/1254)
- node: Future-proof extensions format in Node API [#1155](https://github.com/p2panda/p2panda/pull/1155)
- ci: Improve GitHub actions: Use cargo-deny and cargo-hack, adjust schedule [#1233](https://github.com/p2panda/p2panda/pull/1233)
- stream: Introduce spaces processor [#1218](https://github.com/p2panda/p2panda/pull/1218)

### Changed

- core: Core header type changes & stabilisation
    - `payload_size` and `seq_num` from `u64` to `u32` [#1194](https://github.com/p2panda/p2panda/pull/1194)
    - `version` from `u64` to `u16` [#1194](https://github.com/p2panda/p2panda/pull/1194)
    - Remove `timestamp` [#1195](https://github.com/p2panda/p2panda/pull/1195)
    - Fix header encoding for ZST extensions [#1196](https://github.com/p2panda/p2panda/pull/1196)
- net: Update to iroh `v1.0.0-rc.1` [#1191](https://github.com/p2panda/p2panda/pull/1191)
- net: Update to iroh `v1.0.0` [#1238](https://github.com/p2panda/p2panda/pull/1238)
- net: Use framed postcard codec instead of CBOR for wire protocols [#1198](https://github.com/p2panda/p2panda/pull/1198)
- store: Reduce generics, remove serde from types [#1254](https://github.com/p2panda/p2panda/pull/1254)
- spaces: Only return events when calling Manager::process [#1216](https://github.com/p2panda/p2panda/pull/1216)
- spaces: Don't sync all spaces when group membership changes [#1216](https://github.com/p2panda/p2panda/pull/1216)
- spaces: Adjust all "command" methods to not persist state locally [#1216](https://github.com/p2panda/p2panda/pull/1216)
- spaces: Replace SpacesMessage trait with Borrow<SpacesArgs> [#1217](https://github.com/p2panda/p2panda/pull/1217)
- spaces: Move top-level M generic to Manager::process<M>(..) [#1229](https://github.com/p2panda/p2panda/pull/1229)
- stream: Return input with error in orderer processor [#1249](https://github.com/p2panda/p2panda/pull/1249)
- spaces: Use new traits and SQLite stores in spaces [#1245](https://github.com/p2panda/p2panda/pull/1245)
- chore: Set MSRV to `1.94` [#1191](https://github.com/p2panda/p2panda/pull/1191)
- chore: Set MSRV to `1.96` [#1205](https://github.com/p2panda/p2panda/pull/1205)
- chore: Use assert_matches!() instead of assert!(matches!()) [#1205](https://github.com/p2panda/p2panda/pull/1205)

### Fixed

- encryption: Update hpke-rs to 0.6.1 to fix RUSTSEC [#1233](https://github.com/p2panda/p2panda/pull/1233)
- spaces: Fix generics in MessageStore and MemoryStore [#1229](https://github.com/p2panda/p2panda/pull/1229)
- ci: Check for unused mut and async, fix affected methods [#1163](https://github.com/p2panda/p2panda/pull/1163)
- net: Return existing rx on call to sync manager subscribe [#1269](https://github.com/p2panda/p2panda/pull/1269)
- store: Faulty configuration for in-memory SQLite which caused dropped databases [#1258](https://github.com/p2panda/p2panda/pull/1258)
- store: Do not return stale node infos in address book store [#1285](https://github.com/p2panda/p2panda/pull/1285)

## [0.6.1] - 22/05/2026

### Added

- Emit started & ended events with total operations count when re-playing topic stream [#1175](https://github.com/p2panda/p2panda/pull/1175)
- Show Rust feature flags on docs.rs [#1185](https://github.com/p2panda/p2panda/pull/1185)

### Changed

- Deduplicate `setup_logging` test utility [#1178](https://github.com/p2panda/p2panda/pull/1178)
- Refactor the groups processor to use the `Processor` trait [#1174](https://github.com/p2panda/p2panda/pull/1174)

### Fixed

- Remove pruning logic in LWW TODO example [#1172](https://github.com/p2panda/p2panda/pull/1172)
- Fix premature termination of p2panda-sync stream on duplicate events [#1182](https://github.com/p2panda/p2panda/pull/1182)
- Export missing `DecodeError` and `ReplayError` [#1183](https://github.com/p2panda/p2panda/pull/1183)

## [0.6.0] - 18/05/2026

### Added

- Ensure log transactions as atomic [#1060](https://github.com/p2panda/p2panda/pull/1060)
- Define and implement `Forge` [#1032](https://github.com/p2panda/p2panda/pull/1032)
- Add `LogStore` trait and SQLite implementation [#1004](https://github.com/p2panda/p2panda/pull/1004)
- Add `TopicStore` trait and SQLite implementation [#1011](https://github.com/p2panda/p2panda/pull/1011)
- `Ingest` processor to insert operations and associate them with topic in store [#1044](https://github.com/p2panda/p2panda/pull/1044)
- `as_bytes` method for `Body` [#1044](https://github.com/p2panda/p2panda/pull/1044)
- Event processing pipeline in Node stream [#1045](https://github.com/p2panda/p2panda/pull/1045)
- Replay all operations for a topic based on offset [#1064](https://github.com/p2panda/p2panda/pull/1064)
- Log-prefix pruning processor [#1073](https://github.com/p2panda/p2panda/pull/1073)
- Process local operations [#1080](https://github.com/p2panda/p2panda/pull/1080)
- Process and aggregate metrics for sync events [#1085](https://github.com/p2panda/p2panda/pull/1085)
- Introduce system event API for Node [#1087](https://github.com/p2panda/p2panda/pull/1087)
- Return error when gossip message exceeds maximum size [#1096](https://github.com/p2panda/p2panda/pull/1096)
- Introduce `Author` and use super-trait in `p2panda-core` [#1104](https://github.com/p2panda/p2panda/pull/1104)
- Introduce `Cursor` type in core to track log heights [#1104](https://github.com/p2panda/p2panda/pull/1104)
- `CursorStore` to persist `Cursor` state in SQLite [#1104](https://github.com/p2panda/p2panda/pull/1104)
- Re-play events from any cursor, track acked state [#1104](https://github.com/p2panda/p2panda/pull/1104)
- Processor for groups operations [#1112](https://github.com/p2panda/p2panda/pull/1112)
- `GroupsStore` with `SqliteStore` implementation [#1112](https://github.com/p2panda/p2panda/pull/1112)
- Node API for importing external operation streams [#1135](https://github.com/p2panda/p2panda/pull/1135)
- Option to disable mDNS discovery [#1143](https://github.com/p2panda/p2panda/pull/1143)
- Get current network id from `Node` [#1143](https://github.com/p2panda/p2panda/pull/1143)
- Deduplicate received operations in sync manager event stream [#1147](https://github.com/p2panda/p2panda/pull/1147)
- Todo list example with LWW-CRDT for Node API [#1148](https://github.com/p2panda/p2panda/pull/1148)
- Method for getting filtered groups CRDT heads [#1102](https://github.com/p2panda/p2panda/pull/1102)
- Add PartialEq and Eq derives to auth processor types [#1102](https://github.com/p2panda/p2panda/pull/1165)

### Changed

- Remove `previous` field from `Header` [#1048](https://github.com/p2panda/p2panda/pull/1048)
- Address book SQLite implementation and refactorings [#1007](https://github.com/p2panda/p2panda/pull/1007)
- Remove in-memory store in `p2panda-store-next` and use SQLite in `p2panda-stream-next` [#1016](https://github.com/p2panda/p2panda/pull/1016)
- Use `p2panda-store-next` SQLite stores in `p2panda-net` and `p2panda-sync` [#1022](https://github.com/p2panda/p2panda/pull/1022)
- Use `Topic` everywhere [#1058](https://github.com/p2panda/p2panda/pull/1058)
- More generic API for ingest & operation validation [#1050](https://github.com/p2panda/p2panda/pull/1050)
- Use `Timestamp` in `Header` [#1062](https://github.com/p2panda/p2panda/pull/1062)
- Node API improvements [#1061](https://github.com/p2panda/p2panda/pull/1061)
- Minor store improvements [#1068](https://github.com/p2panda/p2panda/pull/1068)
- Use Borrow to express required args in ingest [#1078](https://github.com/p2panda/p2panda/pull/1078)
- Use MockInstant for Timestamp when running tests [#1081](https://github.com/p2panda/p2panda/pull/1081)
- Derive log id from topic [#1082](https://github.com/p2panda/p2panda/pull/1082)
- Expose insert_bootstrap on the node API [#1125](https://github.com/p2panda/p2panda/pull/1125)
- Update iroh to v0.98.1 [#1131](https://github.com/p2panda/p2panda/pull/1131)
- Forge result is never `None` or duplicate [#1132](https://github.com/p2panda/p2panda/pull/1132)
- Update iroh to v0.98.2 [#1137](https://github.com/p2panda/p2panda/pull/1137)
- Move shared dependencies into workspace Cargo.toml [#1150](https://github.com/p2panda/p2panda/pull/1150)
- Rename core data types and methods [#1158](https://github.com/p2panda/p2panda/pull/1158)

### Fixed

- Fix missing gossip events in sync manager [#988](https://github.com/p2panda/p2panda/pull/988)
- Enforce strictly growing operations log in backlink validation method [#1044](https://github.com/p2panda/p2panda/pull/1044)
- Fix automatic roll-back of unused, dropped permits [#1075](https://github.com/p2panda/p2panda/pull/1075)
- Race where replay_from misses processing ops [#1104](https://github.com/p2panda/p2panda/pull/1104)
- Race where replay state was determined late [#1104](https://github.com/p2panda/p2panda/pull/1104)
- React to node address changes in mDNS test [#1141](https://github.com/p2panda/p2panda/pull/1141)
- Ensure iroh Gossip is shut down gracefully [#1139](https://github.com/p2panda/p2panda/pull/1139)
- Correct "Sync Ended" event semantics in Node API [#1154](https://github.com/p2panda/p2panda/pull/1154)
- Fix header CBOR encoding with correct field count [#1157](https://github.com/p2panda/p2panda/pull/1157)
- Fix issues caused by dependency feature flag changes [#1164](https://github.com/p2panda/p2panda/pull/1164)
- Fix dependency issues for all possible feature-flag combinations [#1168](https://github.com/p2panda/p2panda/pull/1168)

## [0.5.2] - 09/03/2026

### Changed

- `p2panda-auth` remove high-level API and orderer generic [#1030](https://github.com/p2panda/p2panda/pull/1030)A

### Fixed

- Fix SQLite store handling of `previous` hashes [#1051](https://github.com/p2panda/p2panda/pull/1051)
- Fix missing gossip events in sync manager [#988](https://github.com/p2panda/p2panda/pull/988)

## [0.5.1] - 09/02/2026

### Changed

- Update iroh to v0.96.1 [#978](https://github.com/p2panda/p2panda/pull/978)

### Fixed

- Fix Drop impl causing premature gossip unsubscribe [#968](https://github.com/p2panda/p2panda/pull/968)
- Fix panic on sink closure after error during sync session [#972](https://github.com/p2panda/p2panda/pull/972)
- Gracefully handle concurrently deleted operations during sync [#974](https://github.com/p2panda/p2panda/pull/974)
- Cleanup state in gossip unsubscribe handler [#973](https://github.com/p2panda/p2panda/pull/973)
- Fix bind conflict with unique argument seed [#980](https://github.com/p2panda/p2panda/pull/980)
- Fix tests: Subscribe to events before topic stream creation [#981](https://github.com/p2panda/p2panda/pull/981)

## [0.5.0] - 21/01/2026

### Added

- Introduce `Conditions` super-trait in `p2panda-auth` [#780](https://github.com/p2panda/p2panda/pull/780)
- Add `serde` derives to all state structs in `p2panda-auth` [#809](https://github.com/p2panda/p2panda/pull/809)
- `p2panda-spaces`: data encryption for groups and multiple devices [#836](https://github.com/p2panda/p2panda/pull/836) 🥞
- Added `Serialize` and `Deserialize` to `p2panda-spaces::Event` [#868](https://github.com/p2panda/p2panda/pull/868)
- Additional test for framed CBOR operation streaming [#885](https://github.com/p2panda/p2panda/pull/885)
- `p2panda-net` rewrite ([tracking issue](https://github.com/p2panda/p2panda/issues/818))
  - mDNS discovery for iroh endpoint [#869](https://github.com/p2panda/p2panda/pull/869)
  - Simple, incremental backoff logic for random walk [#870](https://github.com/p2panda/p2panda/pull/870)
  - Discovery service connecting address book with iroh [#872](https://github.com/p2panda/p2panda/pull/872)
  - Address book watchers to inform other systems about node info or topic set changes [#876](https://github.com/p2panda/p2panda/pull/876)
  - Trusted transport info [#878](https://github.com/p2panda/p2panda/pull/878)
  - Types, trait and API follow-ups of sync manager integration [#879](https://github.com/p2panda/p2panda/pull/879)
  - Generate NeighborUp events when we join the gossip overlay [#882](https://github.com/p2panda/p2panda/pull/882)
  - Non-blocking manager event streams [#883](https://github.com/p2panda/p2panda/pull/883)
  - Reset backoff when subscribing to topic [#888](https://github.com/p2panda/p2panda/pull/888)
  - Remove transport info when connection attempt failed [#889](https://github.com/p2panda/p2panda/pull/889)
  - Track if node is stale in address book [#891](https://github.com/p2panda/p2panda/pull/891)
  - Hash-based private set intersection for topic discovery [#895](https://github.com/p2panda/p2panda/pull/895) 🥞
  - Improve time-critical tests by using `mock_instant` [#896](https://github.com/p2panda/p2panda/pull/896)
  - Re-initiate sync with node if session fails [#902](https://github.com/p2panda/p2panda/pull/902)
  - Fix ordering for events updating our own transport info [#904](https://github.com/p2panda/p2panda/pull/904)
  - Do not report failed connection attempts when own node has limited reachability [#905](https://github.com/p2panda/p2panda/pull/905)
  - Heal gossip overlay after coming up back online [#906](https://github.com/p2panda/p2panda/pull/906)
  - Add missing relay url in user data during mDNS discovery [#907](https://github.com/p2panda/p2panda/pull/907)
  - Allow endpoint to gracefully shut down on drop [#908](https://github.com/p2panda/p2panda/pull/908)
  - Modular API for new `p2panda-net` [#909](https://github.com/p2panda/p2panda/pull/909) 🥞
  - Method to configure relay urls in Endpoint builder [#948](https://github.com/p2panda/p2panda/pull/948)
  - Chat example for `p2panda-net` using modular API [#929](https://github.com/p2panda/p2panda/pull/929)

### Changed

- One global state object and operation graph for all groups [#781](https://github.com/p2panda/p2panda/pull/781)
- Handle strong removal edge cases and cycles in nested groups, improve concurrent re-adds [#788](https://github.com/p2panda/p2panda/pull/788)
- Make `Extensions` non-optional [#811](https://github.com/p2panda/p2panda/pull/811)
- Add `subscription()` method to `SyncManager` which returns `Stream` [#873](https://github.com/p2panda/p2panda/pull/873)
- Improvements to sync poller actor with tests [#877](https://github.com/p2panda/p2panda/pull/877)
- Do not overwrite serde errors during deserialization of `Header` [#886](https://github.com/p2panda/p2panda/pull/886)
- Module refactoring and minor API improvements in `p2panda-sync` [#944](https://github.com/p2panda/p2panda/pull/944)

### Fixed

- Enable nonblocking for unbound socket used in mdns discovery [#794](https://github.com/p2panda/p2panda/pull/794)
- Allow managing multiple long-term pre-keys [#830](https://github.com/p2panda/p2panda/pull/830)
- Remove uni-streams limit, additional tests for gossip [#874](https://github.com/p2panda/p2panda/pull/874)
- Do not overwrite serde errors during deserialization of `Header` [#886](https://github.com/p2panda/p2panda/pull/886)
- Handle outdated operations which got processed while being pruned, fix overflow substraction bug [#894](https://github.com/p2panda/p2panda/pull/894)
- Gossip handles address book for sync topic [#942](https://github.com/p2panda/p2panda/pull/942)
- Race-condition in add_topic and remove_topic [#947](https://github.com/p2panda/p2panda/pull/947)

## [0.4.0] - 07/07/2025

### Added

- `p2panda-auth`: decentralised group management and authorization [#757](https://github.com/p2panda/p2panda/issues/757) 🥞
- `p2panda-encryption`: decentralized data- and message encryption for groups [#731](https://github.com/p2panda/p2panda/issues/731) 🥞
- Expose sqlite migrations [#744](https://github.com/p2panda/p2panda/pull/744)
- Trait definitions for atomic write transactions [#755](https://github.com/p2panda/p2panda/pull/755)

## [0.3.1] - 14/04/2025

### Added

- Partial ordering algorithm for operations [#710](https://github.com/p2panda/p2panda/pull/710)

### Changed

- Update dependencies (including iroh `v0.34.1`) [#738](https://github.com/p2panda/p2panda/pull/738)

### Fixed

- Reset gossip state to rejoin after major interface change [#726](https://github.com/p2panda/p2panda/pull/726)
- Wait for relay connection initialisation and first direct address [#725](https://github.com/p2panda/p2panda/pull/725)
- Only decrement the gossip buffer counter if it exists and is greater than zero [#722](https://github.com/p2panda/p2panda/pull/722)

## [0.3.0] - 11/03/2025

### Added

- Introduce concrete error type for `SqliteStore` [#698](https://github.com/p2panda/p2panda/pull/698)
- Implement SQLite `OperationStore` & `LogStore` [#680](https://github.com/p2panda/p2panda/pull/680)
- Introduce network system events API [#669](https://github.com/p2panda/p2panda/pull/669)

### Changed

- Refactor sync manager to reduce complexity [#714](https://github.com/p2panda/p2panda/pull/714)
- Expose bootstrap mode setting for network chat example via CLI arg [#709](https://github.com/p2panda/p2panda/pull/709)
- Expand chat example with relay, mdns and bootstrap options [#690](https://github.com/p2panda/p2panda/pull/690)
- Remove logging from network tests [#693](https://github.com/p2panda/p2panda/pull/693)
- Give access to header in `Extension::extract` method [#670](https://github.com/p2panda/p2panda/pull/670)
- Update to iroh `v0.31.0` [#672](https://github.com/p2panda/p2panda/pull/672)
- Update to iroh `v0.33.0` [#707](https://github.com/p2panda/p2panda/pull/707)
- Update to Rust Edition 2024 [#706](https://github.com/p2panda/p2panda/pull/706)

### Fixed

- Deduplicate node addresses in address book [#691](https://github.com/p2panda/p2panda/pull/691)
- Allow bootstrap peer to already enter topic-discovery overlay without any known peers [#688](https://github.com/p2panda/p2panda/pull/688)
- Poll logic causing ingest to never resolve [#697](https://github.com/p2panda/p2panda/pull/697)
- Schedule sync re-attempt after any error occurred [#702](https://github.com/p2panda/p2panda/pull/702)

## [0.2.0] - 20/01/2025

### Changed

- Expose API for setting IPv4 and IPv6 IP and port [#663](https://github.com/p2panda/p2panda/pull/663)
- Re-export gossip config from iroh-gossip [#662](https://github.com/p2panda/p2panda/pull/662)
- Introduce offline-first capability for `LocalDiscovery` mDNS service [#660](https://github.com/p2panda/p2panda/pull/660)
- Update to iroh `v0.28.1` [#661](https://github.com/p2panda/p2panda/pull/661)
- Make log sync protocol bidirectional [#657](https://github.com/p2panda/p2panda/pull/657)
- `TopicMap` replaced by `TopicLogMap` [#650](https://github.com/p2panda/p2panda/pull/650)
- Reset sync and gossip state on major network interface change [#648](https://github.com/p2panda/p2panda/pull/648)
- Remove `Default`, `Sync` and `Send` from `LogId` supertrait definition [#633](https://github.com/p2panda/p2panda/pull/633)

### Fixed

- Fix re-attempt logic for out-of-order buffer in `Ingest` stream [#666](https://github.com/p2panda/p2panda/pull/666)

## [0.1.0] - 06/12/2024

Version `v0.1.0` represents the first release of the new p2panda stack! You can find out more details by reading our [blog](https://p2panda.org/2024/12/06/p2panda-release.html).

[unreleased]: https://github.com/p2panda/p2panda/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/p2panda/p2panda/releases/tag/v0.6.1
[0.6.1]: https://github.com/p2panda/p2panda/releases/tag/v0.6.1
[0.6.0]: https://github.com/p2panda/p2panda/releases/tag/v0.6.0
[0.5.2]: https://github.com/p2panda/p2panda/releases/tag/v0.5.2
[0.5.1]: https://github.com/p2panda/p2panda/releases/tag/v0.5.1
[0.5.0]: https://github.com/p2panda/p2panda/releases/tag/v0.5.0
[0.4.0]: https://github.com/p2panda/p2panda/releases/tag/v0.4.0
[0.3.1]: https://github.com/p2panda/p2panda/releases/tag/v0.3.1
[0.3.0]: https://github.com/p2panda/p2panda/releases/tag/v0.3.0
[0.2.0]: https://github.com/p2panda/p2panda/releases/tag/v0.2.0
[0.1.0]: https://github.com/p2panda/p2panda/releases/tag/v0.1.0
