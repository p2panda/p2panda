<h1 align="center">p2panda-sync</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Data- and transport-agnostic interface to implement custom sync protocol</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
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

## License

Licensed under either of

* Apache License, Version 2.0 ([Apache-2.0.txt](https://github.com/p2panda/p2panda/blob/main/LICENSES/Apache-2.0.txt) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([MIT.txt](https://github.com/p2panda/p2panda/blob/main/LICENSES/MIT.txt) or https://mit-license.org/)

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
p2panda by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions. 
