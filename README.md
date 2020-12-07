# p2panda

All the things a panda needs

* p2panda-rs for using p2panda in your Rust application or library
* p2panda-js for using p2panda in Javascript/Typescript projects

## Requirements

* [Rust](https://www.rust-lang.org/learn/get-started)
* [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

## Usage

Use this in a Webpack 5 setup by enabling the [experimental `syncWebAssembly`
option](https://webpack.js.org/configuration/experiments/).

### Create Ed25519 key pair

```js
import('p2panda-js').then(({ KeyPair }) => {
  // Generates a new Ed25519 key pair using Crypto.randomBytes as
  // cryptographically secure pseudorandom number generator:
  const keyPair = new KeyPair();

  // Returns public and private keys as hex-encoded strings:
  const publicKey = keypair.publicKey();
  const privateKey = keypair.privateKey();

  // Returns public and private keys as byte arrays (Uint8Array):
  const publicKey = keypair.publicKeyBytes();
  const privateKey = keypair.privateKeyBytes();

  // Derive an Ed25519 key pair from a hex-encoded private key:
  const keyPair = KeyPair.fromPrivateKey(privateKey);
});
```

## Development

```bash
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
