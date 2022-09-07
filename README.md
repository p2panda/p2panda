<h1 align="center">p2panda</h1>

<div align="center">
  <strong>All the things a panda needs</strong>
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
    <a href="https://github.com/p2panda/p2panda#installation">
      Installation
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/handbook#how-to-contribute">
      Contributing
    </a>
  </h3>
</div>

<br/>

This library provides all tools required to write a client, node or even your
own protocol implementation for the [`p2panda`] network. It is shipped both as
a Rust crate [`p2panda-rs`] with WebAssembly bindings and a NPM package
[`p2panda-js`] with TypeScript definitions running in NodeJS or any modern web
browser.

The core p2panda [`specification`] is in a stable state but still under review
so please be prepared for breaking API changes until we reach `v1.0`. Currently
no p2panda implementation has recieved a security audit.

[`p2panda`]: https://p2panda.org/
[`p2panda-rs`]: https://github.com/p2panda/p2panda/tree/main/p2panda-rs
[`p2panda-js`]: https://github.com/p2panda/p2panda/tree/main/p2panda-js

## Features

- Generate Ed25519 key pairs.
- Create and encode [`Bamboo`] entries.
- Publish schemas and validate data.
- Create, update and delete data collaboratively.
- Encrypt data with [`OpenMLS`].
- Materialise documents from data changes.
- Prepare data for [`node`] servers.

[`Bamboo`]: https://github.com/AljoschaMeyer/bamboo
[`node`]: https://github.com/p2panda/aquadoggo
[`OpenMLS`]: https://github.com/openmls/openmls

## Usage

```javascript
import { KeyPair } from "p2panda-js";
const keyPair = new KeyPair();
console.log(keyPair.publicKey());
```

```rust
use p2panda_rs::identity::KeyPair;
let key_pair = KeyPair::new();
println!("{}", key_pair.public_key());
```

See [the demo application](https://p2panda.org) and its [source
code](https://github.com/p2panda/zoo-adventures). More examples can be found in the
[`p2panda-rs`] and [`p2panda-js`] directories.

## Installation

If you are using `p2panda` in web browsers or NodeJS applications run:

```bash
$ npm i p2panda-js
```

For Rust environments run:

```bash
$ cargo add p2panda-rs
```

## Development

Visit the corresponding folders for development instructions:
- [`p2panda-rs`](https://github.com/p2panda/p2panda/tree/main/p2panda-rs)
- [`p2panda-js`](https://github.com/p2panda/p2panda/tree/main/p2panda-js)

## Benchmarks

Performance benchmarks can be found in [benches](/p2panda-rs/benchmarks). You
can run them using
[`cargo-criterion`](https://crates.io/crates/cargo-criterion):

```bash
$ cargo install cargo-criterion
$ cargo criterion
# An HTML report with plots is generated automatically
$ open target/criterion/reports/index.html
```

These benchmarks  can be used to compare the performance across branches by
running them first in a base branch and then in the comparison branch. The
HTML-reports will include a comparison of the two results.

## License

GNU Affero General Public License v3.0 [`AGPL-3.0-or-later`](LICENSE)

## Supported by

<img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/ngi-logo.png" width="auto" height="80px"><br />
<img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/eu-flag-logo.png" width="auto" height="80px">

*This project has received funding from the European Union’s Horizon 2020
research and innovation programme within the framework of the NGI-POINTER
Project funded under grant agreement No 871528*
