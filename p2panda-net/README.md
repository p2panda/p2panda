<h1 align="center">p2panda-net</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Data-type-agnostic p2p networking</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

This crate provides a data-type-agnostic p2p networking layer offering robust, direct communication
to any device, no matter where they are.

It provides a stream-based API for higher layers: Applications subscribe to any "topic" they are
interested in and `p2panda-net` will automatically discover similar peers and transport raw bytes
between them.

Additionally `p2panda-net` can be extended with custom sync protocols for all data types, allowing
applications to "catch up on past data", eventually converging to the same state.

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/p2panda/p2panda/blob/main/LICENSES/Apache-2.0.txt)
or [MIT license](https://github.com/p2panda/p2panda/blob/main/LICENSES/MIT.txt) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
p2panda by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions. 
