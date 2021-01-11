# p2panda-js

Use `p2panda-js` to build cool applications using the p2panda protocol running in NodeJS or modern browsers. This library incorporates WebAssembly exported from the sibling `p2panda-rs` project.

## Usage

```js
import p2panda from 'p2panda-js';

async function main() {
  // Wait until libray got initialized
  const { KeyPair } = await p2panda;

  // Generates a new Ed25519 key pair using `Crypto.randomBytes` as
  // cryptographically secure pseudorandom number generator:
  const keyPair = new KeyPair();

  // Returns public and private keys as hex-encoded strings:
  const publicKey = keypair.publicKey();
  const privateKey = keypair.privateKey();

  // Derive an Ed25519 key pair from a hex-encoded private key:
  const keyPairClone = KeyPair.fromPrivateKey(privateKey);
}

main();
```

## Development

For development you will need:

* [Rust](https://www.rust-lang.org/learn/get-started)
* [wasm-pack](https://rustwasm.github.io/wasm-pack/installer)

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
