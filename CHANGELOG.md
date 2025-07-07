# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Highlights are marked with a pancake 🥞

## [Unreleased]

## [0.4.0] - 07/07/2025

### Added

- `p2panda-auth`: decentralised group management and authorization [#757](https://github.com/p2panda/p2panda/issues/757) 🥞
- `p2panda-encryption`: decentralized data- and message encryption for groups [#731](https://github.com/p2panda/p2panda/issues/731) 🥞
- Expose sqlite migrations [#744](https://github.com/p2panda/p2panda/pull/744)

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

[unreleased]: https://github.com/p2panda/p2panda/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/p2panda/p2panda/releases/tag/v0.4.0
[0.3.1]: https://github.com/p2panda/p2panda/releases/tag/v0.3.1
[0.3.0]: https://github.com/p2panda/p2panda/releases/tag/v0.3.0
[0.2.0]: https://github.com/p2panda/p2panda/releases/tag/v0.2.0
[0.1.0]: https://github.com/p2panda/p2panda/releases/tag/v0.1.0
