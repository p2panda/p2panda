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
[`p2panda`]: https://github.com/p2panda/handbook

## Installation

To install `p2panda-js` from the NPM package, run:

```
npm i p2panda-js
```

## Usage

`p2panda-js` uses WebAssembly internally and runs both in NodeJS and web browsers.

### NodeJS

```js
import { createKeyPair } from 'p2panda-js';
const keyPair = createKeyPair();
console.log(keyPair.publicKey());
```

### Browser

To quickly get started you can run `p2panda-js` in any modern browser like that:

```html
<script src="p2panda-js/lib/inline/index.min.js"></script>
<script>
  const { initWebAssembly, createKeyPair } = p2panda;

  async function run() {
    // When running p2panda in the Browser, this method needs to be run once
    // before using all other `p2panda-js` methods
    await initWebAssembly();

    const keyPair = createKeyPair();
    document.getElementById('publicKey').innerText = keyPair.publicKey();
  }

  run();
</script>
<div id="publicKey"></div>
```

The above method includes the WebAssembly inside the JavaScript file (encoded as a base64 string). To load less data you can also manually load the WebAssembly:

```html
<script src="p2panda-js/lib/web/index.min.js"></script>
<script>
  const { initWebAssembly, createKeyPair } = p2panda;

  async function run() {
    // When running p2panda in the Browser, this method needs to be run once
    // before using all other `p2panda-js` methods
    await initWebAssembly('p2panda-js/lib/web/p2panda-js-v0.4.0.wasm');

    const keyPair = createKeyPair();
    document.getElementById('publicKey').innerText = keyPair.publicKey();
  }

  run();
</script>
<div id="publicKey"></div>
```

## React Example

```js
import { createKeyPair, Session } from 'p2panda-js';

// This example uses the "chat" schema at which this hash is pointing. We are
// still working on a good way for you to create and access data schemas. For
// now you can use https://github.com/p2panda/fishyfish to do so
const CHAT_SCHEMA =
  'chat_message_0020a654068b26617ebd6574b1b03853193ccab2295a983bc85a5891793422832655';

// Create a key pair for every usage context of p2panda, i.e. every device and
// every piece of software that is used. Key pairs should never have to be
// transferred between different devices of a user
const keyPair = createKeyPair();

// Open a long running connection to a p2panda node and configure it so all
// calls in this session are executed using that key pair
const session = new Session('https://welle.liebechaos.org').setKeyPair(keyPair);

// Compose your operation payload, according to chosen schema
const payload = {
  message: 'Hi there',
};

// Send new chat operation to the node
await session.create(payload, { schema: CHAT_SCHEMA });

// Query instances from the p2panda node
import { gql, useQuery } from '@apollo/client';

const GET_CHAT_MESSAGES = gql`
  all_${CHAT_SCHEMA} {
    fields {
      message
    }
  }
`;

const Chat = () => {
  const { loading, error, data } = useQuery(GET_CHAT_MESSAGES);

  if (loading) return 'Loading...';
  if (error) return `Error! ${error.message}`;

  return (
    <ul>
      {data[`all_${CHAT_SCHEMA}`].map((doc) => (
        <li key={doc.id}>{doc.fields.message}</li>
      ))}
    </ul>
  );
};
```

## Development

### Dependencies

- [`NodeJS`](https://nodejs.org/en/)
- [`Rust`](https://www.rust-lang.org/learn/get-started)
- [`wasm-bindgen`](https://rustwasm.github.io/wasm-bindgen/reference/cli.html)
- [`wasm-opt`](https://github.com/WebAssembly/binaryen/discussions/3797)

In order to develop with the current code base `p2panda-js` needs to be compiled from the [`p2panda-rs`](https://github.com/p2panda/p2panda/tree/main/p2panda-rs) code using `wasm-pack`. This requires a working `Rust` environment to be setup and `wasm-bindgen` to be installed. `wasm-opt` is only required to optimize the WebAssembly builds for production via `npm run build`. You can then run the following commands, the compilation occurs during the testing and build phases:

```bash
# Install dependencies
npm install

# Check code formatting
npm run lint

# Run tests, requires `wasm-bindgen`
npm test

# Compile wasm and bundle js package, requires `wasm-bindgen` and `wasm-opt`
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

### Documentation

```bash
# Generate documentation
npm run docs

# Show documentation in browser
npx serve ./docs
```

## License

GNU Affero General Public License v3.0 [`AGPL-3.0-or-later`](LICENSE)

## Supported by

<img src="https://p2panda.org/images/ngi-logo.png" width="auto" height="80px"><br />
<img src="https://p2panda.org/images/eu-flag-logo.png" width="auto" height="80px">

*This project has received funding from the European Union’s Horizon 2020
research and innovation programme within the framework of the NGI-POINTER
Project funded under grant agreement No 871528*
