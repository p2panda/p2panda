<h1 align="center">p2panda-sync</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Data- and transport-agnostic sync protocols</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-sync">
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

This crate provides a data- and transport-agnostic interface to implement custom sync protocols,
compatible with `p2panda-net` or other peer-to-peer networking solutions.

In addition to the generic definition of the `SyncProtocol` trait, `p2panda-sync` includes
optional implementations for efficient sync of append-only log-based data types. These optional
implementations may be activated via feature flags. Finally, `p2panda-sync` provides helpers to
encode wire messages in CBOR.

## Features

- Transport- and data-type agnostic trait definitions compatible with `p2panda-net`
- Efficient and ready-to-use implementation for log-height based sync of p2panda core data-types
- Privacy-first design allowing implementations to reveal as little information as possible during handshake phase
- Generic design to re-use the same sync protocol for very different applications

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
