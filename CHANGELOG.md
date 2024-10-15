# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Highlights are marked with a pancake 🥞

## [Unreleased]

### Added

- Move discovery from `p2panda-net` into new `p2panda-discovery` crate [#571](https://github.com/p2panda/p2panda/pull/571) 
- Replace `Ready` network message with oneshot receiver on subscribe [#570](https://github.com/p2panda/p2panda/pull/570)
- Refactor sync connection manager [#563](https://github.com/p2panda/p2panda/pull/563)
- Bump `iroh` dependencies to `0.25.0` [#562](https://github.com/p2panda/p2panda/pull/562)
- Implement sync connection manager [#559](https://github.com/p2panda/p2panda/pull/559)
- Introduce `TopicMap` trait [#560](https://github.com/p2panda/p2panda/pull/560)
- Sync past state for subscribed topics in `p2panda-net` [#553](https://github.com/p2panda/p2panda/pull/553)
- Make all store methods async and use interior mutability patterns [#550](https://github.com/p2panda/p2panda/pull/550)
- Introduce `p2panda-sync` offering generic sync tools and opinionated sync protocols [#549](https://github.com/p2panda/p2panda/pull/549)
- Introduce blobs functionality, including import, export and download
  [#546](https://github.com/p2panda/p2panda/pull/546)
- Bump `iroh` dependencies to `0.22.0` [#543](https://github.com/p2panda/p2panda/pull/543)
- Introduce networking functionality, including discovery and gossip
  services [#540](https://github.com/p2panda/p2panda/pull/540)
- Introduce all core p2panda types [#535](https://github.com/p2panda/p2panda/pull/535)
- Introduce basic storage traits w/ MemoryStore implementation [#536](https://github.com/p2panda/p2panda/pull/536)

### Changed

- Handle non-copy store method parameters by reference [#558](https://github.com/p2panda/p2panda/pull/558)
- Handle operation header and body as bytes in log height sync [#561](https://github.com/p2panda/p2panda/pull/561)
