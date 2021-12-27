<h1 align="center">p2panda-rs</h1>

<div align="center">
  <strong>All the things a panda needs (for Rust)</strong>
</div>

<br />

<div align="center">
  <!-- CI status -->
  <a href="https://github.com/p2panda/p2panda/actions">
    <img src="https://img.shields.io/github/workflow/status/p2panda/p2panda/Build%20and%20test?style=flat-square" alt="CI Status" />
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
    <a href="https://github.com/p2panda/p2panda">
      Installation
    </a>
    <span> | </span>
    <a href="https://docs.rs/p2panda-rs">
      API
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

<br />

This library provides all tools required to write a client for the [`p2panda`] network. It is shipped both as a Rust crate [`p2panda-rs`] with WebAssembly bindings and a NPM package [`p2panda-js`] with TypeScript definitions running in NodeJS or any modern web browser.

Read the library [`documentation`] for installation guides and examples.

[`documentation`]: https://github.com/p2panda/p2panda
[`p2panda-js`]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
[`p2panda-rs`]: https://github.com/p2panda/p2panda/tree/main/p2panda-rs
[`p2panda`]: https://github.com/p2panda/handbook

## Development

You will need the following tools to start development:

- [Rust](https://www.rust-lang.org/learn/get-started)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

```bash
# Run tests
cargo test

# Compile wasm as npm browser package into `pkg` folder
wasm-pack build
```

## Testing

`p2panda-rs` provides tools which can be used for testing your code. Read more about the `test_utils` module [here](/p2panda-rs/src/test_utils/README.md).

## License

GNU Affero General Public License v3.0 [`AGPL-3.0-or-later`](LICENSE)

## Supported by

<img src="https://p2panda.org/images/ngi-logo.png" width="auto" height="80px"><br /><img src="https://p2panda.org/images/eu-flag-logo.png" width="auto" height="80px">

*This project has received funding from the European Union’s Horizon 2020 research and innovation programme within the framework of the NGI-POINTER Project funded under grant agreement No 871528*
