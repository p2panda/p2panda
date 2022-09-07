<h1 align="center">p2panda-js</h1>

<div align="center">
  <strong>All the things a panda needs (for JavaScript)</strong>
</div>

<br />

<div align="center">
  <!-- CI status -->
  <a href="https://github.com/p2panda/p2panda/actions">
    <img src="https://img.shields.io/github/checks-status/p2panda/p2panda/main?style=flat-square" alt="CI Status" />
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
    <a href="https://p2panda.org/lib/p2panda-js">
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
    <span> | </span>
    <a href="https://p2panda.org">
      Website
    </a>
  </h3>
</div>

<br />

This library provides all tools required to write a client, node or even your own protocol implementation for the [`p2panda`] network. It is shipped both as a Rust crate [`p2panda-rs`] with WebAssembly bindings and a NPM package [`p2panda-js`] with TypeScript definitions running in NodeJS or any modern web browser.

> In the future `p2panda-js` will have full feature parity with `p2panda-rs` to be able to write high-level client frameworks or node implementations in TypeScript. Until now `p2panda-js` provides basic methods to create, sign and encode data.

[`p2panda-js`]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
[`p2panda-rs`]: https://github.com/p2panda/p2panda/tree/main/p2panda-rs
[`p2panda`]: https://p2panda.org

## Installation

To install `p2panda-js` from the NPM package, run:

```
npm i p2panda-js
```

## Usage

`p2panda-js` runs both in NodeJS and web browsers and comes as a ES, CommonJS or UMD module. It can easily be integrated into Webpack, Rollup or other tools.

Since `p2panda-js` contains WebAssembly code, it is necessary to initialise it before using the methods in the Browser. This initialisation step is not required in NodeJS contexts.

To make this step a little bit easier `p2panda-js` inlines the WebAssembly code as a base64 string which gets decoded automatically during initialisation. For manual initialisation the package also comes with "slim" versions where you need to provide a path to the ".wasm" file yourself, you can read about this approach [further below](#manually-load-wasm).

### NodeJS

```javascript
import { KeyPair } from 'p2panda-js';
const keyPair = new KeyPair();
console.log(keyPair.publicKey());
```

### Browser

To quickly get started you can run `p2panda-js` in any modern browser as an ES module like that:

```html
<script type="module">
  import { initWebAssembly, KeyPair } from 'https://cdn.jsdelivr.net/npm/p2panda-js@0.5.0/lib/esm/index.min.js';

  // This only needs to be done once before using all `p2panda-js` methods.
  initWebAssembly().then(() => {
    const keyPair = new KeyPair();
    console.log(keyPair.publicKey());
  });
</script>
```

Or use the "slim" version if you want to provide the ".wasm" file manually:

```html
<script type="module">
  import { initWebAssembly, KeyPair } from 'https://cdn.jsdelivr.net/npm/p2panda-js@0.5.0/lib/esm-slim/index.min.js';

  // Pass external .wasm file manually for smaller file sizes
  const wasmFile = 'https://cdn.jsdelivr.net/npm/p2panda-js@0.5.0/lib/p2panda.wasm';
  initWebAssembly(wasmFile).then(() => {
    const keyPair = new KeyPair();
    console.log(keyPair.publicKey());
  });
</script>
```

### Bundlers

```javascript
import { initWebAssembly, KeyPair } from 'p2panda-js';

// This only needs to be done once before using all `p2panda-js` methods.
await initWebAssembly();

const keyPair = new KeyPair();
console.log(keyPair.publicKey());
```

### Manually load `.wasm`

Running `p2panda-js` in the browser automatically inlines the WebAssembly inside the JavaScript file, encoded as a base64 string. While this works for most developers, it also doubles the size of the imported file. To avoid larger payloads and decoding times you can load the `.wasm` file manually by using a "slim" version. For this you need to initialise the module by passing the path to the file into `initWebAssembly`:

```javascript
// Import from `slim` module to manually initialise WebAssembly code
import { initWebAssembly, KeyPair } from 'p2panda-js/slim';
import wasm from 'p2panda-js/p2panda.wasm';

// When running p2panda in the browser, this method needs to run once
// before using all other `p2panda-js` methods
await initWebAssembly(wasm);

const keyPair = new KeyPair();
console.log(keyPair.publicKey());
```

## Development

### Dependencies

- [`NodeJS`](https://nodejs.org/en/)
- [`Rust`](https://www.rust-lang.org/learn/get-started)
- [`wasm-bindgen`](https://rustwasm.github.io/wasm-bindgen/reference/cli.html)
- [`wasm-opt`](https://github.com/WebAssembly/binaryen/discussions/3797)

In order to develop with the current code base `p2panda-js` needs to be compiled from the [`p2panda-rs`](https://github.com/p2panda/p2panda/tree/main/p2panda-rs) code using `wasm-bindgen`. This requires a working `Rust` environment to be setup and `wasm-bindgen` to be installed. `wasm-opt` is only required to optimize the WebAssembly builds for production via `npm run build`. You can then run the following commands, the compilation occurs during the testing and build phases:

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

<img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/ngi-logo.png" width="auto" height="80px"><br />
<img src="https://raw.githubusercontent.com/p2panda/.github/main/assets/eu-flag-logo.png" width="auto" height="80px">

*This project has received funding from the European Union’s Horizon 2020 research and innovation programme within the framework of the NGI-POINTER Project funded under grant agreement No 871528*
