<h1 align="center">p2panda</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Out-of-the-box p2panda API for application developers</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda">
      Documentation
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

p2panda's high-level API is an opiniated, out-of-the-box peer-to-peer stack which orchestrates
all individual [p2panda] modules.

```rust
let topic = Topic::random();
let node = p2panda::spawn().await?;
let (tx, rx) = node.stream(topic).await?;
```

It provides peer-to-peer networking, discovery, bootstrap, local-first sync, event streaming, causal
ordering, storage, and more in one easy-to-use API.

> 🚧 This library is under active development and the APIs are not yet considered stable for
> production use. Core data types and user-facing APIs may still undergo breaking changes. Stability
> guarantees will improve with the release of v1.0.0.

## Features

- High-level p2panda API for building decentralised p2p and [local-first] applications with
  minimal setup
- Unified orchestration of p2p networking, node discovery, mDNS, bootstrap, [eventually consistent]
  sync, event streaming, causal ordering, pruning, and persistence
- Topic-based [Publish & Subscribe] model with partial replication - sync only the data relevant to
  a topic
- Transport-agnostic "event delivery" architecture supporting Internet p2p today (QUIC/iroh) and
  future mesh/radio transports such as BLE and LoRa
- Built on single-writer append-only, fork-resistant CRDT operation logs with pruning, multi-writer
  causal ordering, and efficient sync
- Persistent local SQLite storage for operations, sync state, address books, stream cursors, and
  soon encryption/access-control state
- Event-stream-inspired consumer model with acknowledgements, replay support, at-least-once delivery
  semantics, and crash recovery
- Atomic transactional processing pipeline for resilience against crashes and corrupted database
  state
- Observable system state, events, and metrics

## Getting Started

Install the Rust crate using `cargo add p2panda` and read our [documentation] and [examples] folder
for an introduction and code examples of the API.

## Other languages / FFI

There's experimental support for bindings of p2panda's API into alternative programming languages
and flavours:

- [`p2panda-gobject`] Introspectable GLib/GObject API for various languages
- [`p2panda-ffi`] Node.js, Python and Go support via UniFFI

## Roadmap

- Out-of-the-box [Tor support] to protect IP addresses
- Filterable topic streams
- Integration of Capabilities, like [Meadowcap] or [UCAN]
- [Multi-device group management] with revocation
- [Encryption for groups] with Forward Secrecy
- [Support Nodes] to assure higher availability
- Event delivery via [delay tolerant, replicated mesh-routing] for BLE and LoRa

[Encryption for groups]: https://p2panda.org/2025/02/24/group-encryption.html
[Meadowcap]: https://willowprotocol.org/specs/meadowcap/index.html
[Multi-device group management]: https://p2panda.org/2025/07/28/access-control.html
[Publish & Subscribe]: https://en.wikipedia.org/wiki/Publish%E2%80%93subscribe_pattern
[Support Nodes]: https://fosdem.org/2026/schedule/event/MCVBNK-p2panda-modal-reflection/
[Tor support]: https://www.torproject.org
[UCAN]: https://ucan.xyz/
[`p2panda-ffi`]: https://github.com/p2panda/p2panda-ffi
[`p2panda-gobject`]: https://github.com/p2panda/p2panda-gobject
[delay tolerant, replicated mesh-routing]: https://en.wikipedia.org/wiki/Delay-tolerant_networking
[documentation]: https://docs.rs/p2panda
[eventually consistent]: https://en.wikipedia.org/wiki/Eventual_consistency
[examples]: https://github.com/p2panda/p2panda/tree/main/p2panda/examples
[local-first]: https://www.inkandswitch.com/local-first-software/
[p2panda]: https://p2panda.org

## License

Licensed under either of [Apache License, Version 2.0] or [MIT license] at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
p2panda by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[Apache License, Version 2.0]: https://github.com/p2panda/p2panda/blob/main/LICENSES/Apache-2.0.txt
[MIT license]: https://github.com/p2panda/p2panda/blob/main/LICENSES/MIT.txt

---

_This project has received funding from the European Union’s Horizon 2020 research and innovation
programme within the framework of the NGI-POINTER Project funded under grant agreement No 871528,
NGI-ASSURE No 957073, NGI0-ENTRUST No 101069594 and NGI0-COMMONS No 101135429._
