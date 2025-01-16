# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Highlights are marked with a pancake 🥞

## [Unreleased]

### Changed

- Introduce offline-first capability for `LocalDiscovery` mDNS service [#660](https://github.com/p2panda/p2panda/pull/660)
- Update to iroh `v0.28.1` [#661](https://github.com/p2panda/p2panda/pull/661)
- Make log sync protocol bidirectional [#657](https://github.com/p2panda/p2panda/pull/657)
- `TopicMap` replaced by `TopicLogMap` [#650](https://github.com/p2panda/p2panda/pull/650)
- Reset sync and gossip state on major network interface change [#648](https://github.com/p2panda/p2panda/pull/648)
- Remove `Default`, `Sync` and `Send` from `LogId` supertrait definition [#633](https://github.com/p2panda/p2panda/pull/633)

## [0.1.0]

Version `v0.1.0` represents the first release of the new p2panda stack! You can find out more details by reading our [blog](https://p2panda.org/2024/12/06/p2panda-release.html).

[unreleased]: https://github.com/p2panda/p2panda/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/p2panda/p2panda/releases/tag/v0.1.0
