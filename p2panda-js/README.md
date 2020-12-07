# p2panda-js

Use `p2panda-js` to build cool applications using the p2panda protocol. This
library incorporates web assembly exported from the sibling `p2panda-rs` project.
Use webpack to build and combine *everything*!

## Requirements

* [Node.js](https://nodejs.org)
* [Rust](https://www.rust-lang.org/learn/get-started)
* [wasm-pack](https://rustwasm.github.io/wasm-pack/installer)

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
# Install dependencies
npm install

# Check code formatting
npm run lint

# Run tests
npm test

# Compile to wasm and bundle js package
npm run build
```

## License

GNU Affero General Public License v3.0 `AGPL-3.0`
