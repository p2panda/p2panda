<h1 align="center">p2panda</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>All the things a panda needs</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

p2panda aims to provide everything you need to build modern, privacy-respecting and secure local-first applications.

We have adopted a modular approach—allowing projects the freedom to pick what they need and integrate it with minimal friction. We believe this approach contributes the most to a wider, interoperable p2p ecosystem which outlives “framework lock-in”.

Many of our Rust crates operate over raw bytes and are fully compatible with your own data types and any CRDT. In case you don't plan on building your own peer-to-peer protocol, we have you covered with all features required to build a mobile or desktop application.

We're using existing libraries like [iroh](https://github.com/n0-computer/iroh) and well-established standards such as BLAKE3, Ed25519, STUN, CBOR, TLS, QUIC, [UCAN](https://github.com/ucan-wg/spec), [Double Ratchet](https://en.m.wikipedia.org/wiki/Double_Ratchet_Algorithm) and more - as long as they give us the radical offline-first guarantee we need.

We want collaboration, encryption and access-control to work even when operating over unstable or ephemeral connections. Towards this end, we're actively working alongside researchers to design and implement resilient solutions.

p2panda is "broadcast-only" at it’s heart, making any data not only offline-first but also compatible with post-internet communication infrastructure, such as shortwave, packet radio, Bluetooth Low Energy, LoRa or simply a USB stick.

## Libraries

📦 [`p2panda-net`](https://crates.io/crates/p2panda-net) - Find peers in a peer-to-peer network, connect to them directly - wherever they are - and exchange any data of your interest in form of byte streams.

📦 [`p2panda-discovery`](https://crates.io/crates/p2panda-discovery) - Solutions to find other peers in your local network or on the internet and interfaces to start building your own.

📦 [`p2panda-sync`](https://crates.io/crates/p2panda-sync) - Protocol implementations to efficiently "catch up on past state" with other peers and interfaces to start building your own.

📦 [`p2panda-blobs`](https://crates.io/crates/p2panda-blobs) - Efficiently send, receive and store (very large) files.

📦 [`p2panda-core`](https://crates.io/crates/p2panda-core) - Highly extensible data-types of the p2panda protocol for secure, distributed and efficient exchange of data, supporting networks from the internet to packet radio, LoRa or BLE.

📦 [`p2panda-store`](https://crates.io/crates/p2panda-store) - Interfaces and implementations to store p2panda data types in databases, memory or file-systems.

📦 [`p2panda-stream`](https://crates.io/crates/p2panda-stream) - Collection of various methods to process your p2panda data streams before they reach your application.

📦 [`p2panda-encryption`](https://crates.io/crates/p2panda-encryption) - Decentralised data- and message encryption for groups with post-compromise security and optional forward secrecy.

📦 [`p2panda-auth`](https://crates.io/crates/p2panda-auth) - Decentralised group management with fine-grained, per-member permissions.

🚧  `p2panda-node` - All-in-one p2panda node which can be used in federated or fully decentralised networks or both at the same time. Supports "lightweight" clients running in the browser.

## License

Licensed under either of [Apache License, Version 2.0] or [MIT license] at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
p2panda by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[Apache License, Version 2.0]: https://github.com/p2panda/p2panda/blob/main/LICENSES/Apache-2.0.txt
[MIT license]: https://github.com/p2panda/p2panda/blob/main/LICENSES/MIT.txt

---

*This project has received funding from the European Union’s Horizon 2020
research and innovation programme within the framework of the NGI-POINTER
Project funded under grant agreement No 871528, NGI-ASSURE No 957073 and
NGI0-ENTRUST No 101069594*.

[`p2panda`]: https://p2panda.org
