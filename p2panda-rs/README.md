<h1 align="center">p2panda-rs</h1>

<div align="center">
  <strong>All the things a panda needs (for Rust)</strong>
</div>

<br />

<div align="center">
  <!-- CI status -->
  <a href="https://github.com/p2panda/p2panda/actions">
    <img src="https://img.shields.io/github/checks-status/p2panda/p2panda/main?style=flat-square" alt="CI Status" />
  </a>
  <!-- Codecov report -->
  <a href="https://app.codecov.io/gh/p2panda/p2panda/">
    <img src="https://img.shields.io/codecov/c/gh/p2panda/p2panda?style=flat-square" alt="Codecov Report" />
  </a>
  <!-- Crates version -->
  <a href="https://crates.io/crates/p2panda-rs">
    <img src="https://img.shields.io/crates/v/p2panda-rs.svg?style=flat-square" alt="Crates.io version" />
  </a>
  <!-- NPM version -->
  <a href="https://www.npmjs.com/package/p2panda-js">
    <img src="https://img.shields.io/npm/v/p2panda-js?style=flat-square" alt="NPM version" />
  </a>
</div>

<div align="center">
  <h3>
    <a href="https://docs.rs/p2panda-rs">
      Docs
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://p2panda.org/about/contribute">
      Contribute
    </a>
    <span> | </span>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

<br />

This library provides all tools required to write a client, node or even your
own protocol implementation for the [`p2panda`] network. It is shipped both as
a Rust crate [`p2panda-rs`] with WebAssembly bindings and a NPM package
[`p2panda-js`] with TypeScript definitions running in NodeJS or any modern web
browser.

> The core p2panda [specification](https://p2panda.org/specification/) is in a 
stable state but still under review so please be prepared for breaking API 
changes until we reach `v1.0`. Currently no p2panda implementation has recieved 
a security audit.

[`documentation`]: https://github.com/p2panda/p2panda
[`p2panda-js`]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
[`p2panda-rs`]: https://github.com/p2panda/p2panda/tree/main/p2panda-rs
[`p2panda`]: https://p2panda.org

## Installation

With [`cargo-edit`](https://github.com/killercup/cargo-edit) installed run:

```bash
cargo add p2panda-rs
```

## Example

```rust
use p2panda_rs::entry::encode::encode_entry;
use p2panda_rs::entry::EntryBuilder;
use p2panda_rs::identity::KeyPair;
use p2panda_rs::operation::encode::encode_operation;
use p2panda_rs::operation::OperationBuilder;

// Id of the schema which describes the data we want to publish. This should
// already be known to the node we are publishing to.
pub const SCHEMA_ID_STR: &str =
    "profile_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

    // Generate new Ed25519 key pair.
let key_pair = KeyPair::new();

// Add field data to "create" operation.
let operation = OperationBuilder::new(&SCHEMA_ID_STR.parse()?)
    .fields(&[("username", "panda".into())])
    .build()?;

// Encode operation into bytes.
let encoded_operation = encode_operation(&operation)?;

// Create Bamboo entry (append-only log data type) with operation as payload.
let entry = EntryBuilder::new().sign(&encoded_operation, &key_pair)?;

// Encode entry into bytes.
let encoded_entry = encode_entry(&entry)?;

println!("{} {}", encoded_entry, encoded_operation);
```

To run this example from the `examples/` folder like so:

```bash
cargo run --example=readme
```

## Development

You will need the following tools to start development:
- [Rust](https://www.rust-lang.org/learn/get-started)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

```bash
# Run tests
cargo test

# Run WebAssembly tests
wasm-pack test --headless --firefox
```

## License

GNU Affero General Public License v3.0 [`AGPL-3.0-or-later`](LICENSE)

## Supported by

<img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/ngi-logo.png" width="auto" height="80px"><br />
<img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/eu-flag-logo.png" width="auto" height="80px">

*This project has received funding from the European Union’s Horizon 2020
research and innovation programme within the framework of the NGI-POINTER
Project funded under grant agreement No 871528*
