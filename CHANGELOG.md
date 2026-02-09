# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Highlights are marked with a pancake 🥞

## [Unreleased]

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

[unreleased]: https://github.com/p2panda/p2panda/compare/v0.5.1...HEAD
[0.5.1]: https://github.com/p2panda/p2panda/releases/tag/v0.5.1
[0.5.0]: https://github.com/p2panda/p2panda/releases/tag/v0.5.0
[0.4.0]: https://github.com/p2panda/p2panda/releases/tag/v0.4.0
[0.3.1]: https://github.com/p2panda/p2panda/releases/tag/v0.3.1
[0.3.0]: https://github.com/p2panda/p2panda/releases/tag/v0.3.0
[0.2.0]: https://github.com/p2panda/p2panda/releases/tag/v0.2.0
[0.1.0]: https://github.com/p2panda/p2panda/releases/tag/v0.1.0
