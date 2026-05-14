<h1 align="center">p2panda-store</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Database traits and SQLite implementations with support for atomic transactions</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-store">
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

This crate provides generic trait definitions to flexibly express storage and query behaviour for a
wide-range of peer-to-peer systems. In the context of p2panda these include an address book for
managing transport information related to nodes in a network, an operation store for maintaining
append-only log entries, an orderer store to track operation dependencies, and much more. Concrete
SQLite database implementations are provided for all store traits, along with a transaction provider
for cases when atomicity and consistency are required for a set of database interactions.

> 🚧 This library is under active development and the APIs are not yet considered stable for
> production use. Core data types and user-facing APIs may still undergo breaking changes. Stability
> guarantees will improve with the release of v1.0.0.

## Features

- Generic trait definitions required to implement p2panda stores
- SQLite implementations for all p2panda stores
  - Address book for handling node information
  - Cursors for tracking positions in logs
  - Groups for maintaining auth group state
  - Logs for efficient comparison of log-based data types
  - Operations for storing entries in append-only logs
  - Orderer for tracking dependencies in partially-ordered data sets
- Transaction provider to group related queries for consistency guarantees
- Database migrations on store creation or during application runtime

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
