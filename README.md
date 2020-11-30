# sesamoid

Utility library for p2panda applications.

## Requirements

* [Rust](https://www.rust-lang.org/learn/get-started)
* [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

## Usage

Use this in a Webpack 5 setup by enabling the [experimental `syncWebAssembly`
option](https://webpack.js.org/configuration/experiments/).

Create a key pair:

```
import('sesamoid').then(({ KeyPair }) => {
  const keyPair = new KeyPair();
  const public = keypair.publicKeyBytes(); // UInt8Array
  const private = keypair.privateKeyBytes();
});
```

## Development

```
# Run tests
cargo test

# Compile wasm as npm browser package into `pkg` folder
wasm-pack build

# Link package for usage in other projects during development
cd pkg
npm link
```

## License

GNU Affero General Public License v3.0 `AGPL-3.0`
