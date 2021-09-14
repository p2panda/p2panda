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
    <a href="https://github.com/p2panda/design-document#how-to-contribute">
      Contributing
    </a>
  </h3>
</div>

<br />

This library provides all tools required to write a client for the [`p2panda`] network. It is shipped both as a Rust crate [`p2panda-rs`] with WebAssembly bindings and a NPM package [`p2panda-js`] with TypeScript definitions running in NodeJS or any modern web browser.

[`p2panda-js`]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
[`p2panda-rs`]: https://github.com/p2panda/p2panda/tree/main/p2panda-rs
[`p2panda`]: https://github.com/p2panda/design-document

## Installation

To install `p2panda-js` from the NPM package, run:

```
npm i p2panda-js
```

## Usage

```js
import { createKeyPair, Session } from 'p2panda-js';

// This example uses the "chat" schema at which this hash is pointing. We are
// still working on a good way for you to create and access data schemas. For
// now you can use https://github.com/p2panda/fishyfish to do so
const CHAT_SCHEMA =
  '00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906' +
  '401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437';

// Create a key pair for every usage context of p2panda, i.e. every device and
// every piece of software that is used. Key pairs should never have to be
// transferred between different devices of a user
const keyPair = await createKeyPair();

// Open a long running connection to a p2panda node and configure it so all
// calls in this session are executed using that key pair
const session = new Session('https://welle.liebechaos.org').setKeyPair(keyPair);

// Compose your message payload, according to chosen schema
const payload = {
  message: 'Hi there',
};

// Send new chat message to the node
await session.create(payload, { schema: CHAT_SCHEMA });

// Query instances from the p2panda node
const instances = await session.query({ schema: CHAT_SCHEMA });
console.log(instances[0].message);
// -> 'Hi there'
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

### Debug logging

Enable debug logging for node environments by setting an environment variable:

```bash
export DEBUG='p2panda*'
```

Enable debug logging from a browser console by storing a key `debug` in local storage:

```js
localStorage.debug = 'p2panda*';
```

## License

GNU Affero General Public License v3.0 [`AGPL-3.0-or-later`](LICENSE)
