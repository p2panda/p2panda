<h1 align="center">p2panda-js</h1>

<div align="center">
  <strong>All the things a panda needs (for JavaScript)</strong>
</div>

<br />

<div align="center">
  <!-- CI status -->
  <a href="https://github.com/p2panda/p2panda/actions">
    <img src="https://img.shields.io/github/workflow/status/p2panda/p2panda/Build%20and%20test?style=flat-square" alt="CI Status" />
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
    <a href="https://github.com/p2panda/p2panda/releases">
      Releases
    </a>
    <span> | </span>
    <a href="https://github.com/p2panda/design-document#get-involved">
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
[`p2panda`]: https://github.com/p2panda/design-document

## Installation

To install `p2panda-js` from the NPM package, simply run:

`npm i p2panda-js`

## Usage

Create a key pair for each device and user who need to access p2panda.

```
import { KeyPair } from 'p2panda-js';
const keyPair = new KeyPair();
```

Create an instance using an already known schema for chat messages.

```
import { Session } from 'p2panda-js';

const session = new Session('https://welle.liebechaos.org')
  .keyPair(keyPair);

const payload = {
  message: 'Hi there'
}
const entry = await session.create(payload, { schema: CHAT_SCHEMA })

```

## Development Setup

### Dependencies

- [`NodeJS`](https://nodejs.org/en/)
- [`Rust`](https://www.rust-lang.org/learn/get-started)
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/)

In order to develop with the current code base `p2panda-js` needs to be compiled from the [`p2panda-rs`](https://github.com/p2panda/p2panda/tree/main/p2panda-rs) code using `wasm-pack`. This requires a working `Rust` environment to be setup and `wasm-pack` to be installed. You can then run the following commands, the compilation occurs during the testing and build phases.

```bash
# Install dependencies
npm install

# Check code formatting
npm run lint

# Run tests
npm test

# Compile wasm and bundle js package
npm run build
```

## License

GNU Affero General Public License v3.0 `AGPL-3.0`
