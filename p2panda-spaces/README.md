<h1 align="center">p2panda-spaces</h1>

<div align="center">
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-left.gif" width="auto" height="30px">
  <strong>Data encryption for groups and multiple devices</strong>
  <img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/panda-right.gif" width="auto" height="30px">
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-spaces">
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

This crate provides an API for establishing and managing encryption contexts with dynamic groups
of actors. The p2panda-encryption [Data
Encryption](https://docs.rs/p2panda-encryption/latest/p2panda_encryption/data_scheme/index.html)
scheme is used for key agreement and group management is achieved through an integration with
[p2panda-auth groups](https://docs.rs/p2panda-auth/latest/p2panda_auth/). The main entry point for
users is the `Manager` struct from which groups and spaces can be created.

## Features

* Decentralised group key agreement with forward secrecy and encrypted messaging with
  post-compromise security
* Decentralised group management with robust conflict resolution strategies
* Private space identifiers
* Re-use of groups across encryption boundaries
* Nested groups allowing for modelling multi-device profiles
* Generic over message type

## Requirements

* Messages must be ordered according to causal relations
* Messages must be signed and verified

Read more about the underlying [groups CRDT](https://docs.rs/p2panda-auth/latest/p2panda_auth/)
and [encryption scheme](https://docs.rs/p2panda-encryption/latest/p2panda_encryption/).

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
