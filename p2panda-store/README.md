<h1 align="center">p2panda-store</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Persistence layer interfaces and implementations for core p2panda data types</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

This crate provides APIs to allow for efficient implementations of operation and log stores. These
persistence and query APIs are utilised by higher-level components of the p2panda stack, such
as `p2panda-sync` and `p2panda-stream`. 

An in-memory storage solution is provided in the form of a `MemoryStore` which implements both the
`OperationStore` and `LogStore` traits.

## License

Licensed under either of [Apache License, Version 2.0](https://github.com/p2panda/p2panda/blob/main/LICENSES/Apache-2.0.txt)
or [MIT license](https://github.com/p2panda/p2panda/blob/main/LICENSES/MIT.txt) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
p2panda by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
