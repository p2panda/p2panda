<h1 align="center">p2panda-js</h1>

<div align="center">
  <strong>All the things a panda needs (for JavaScript)</strong>
</div>

<br/>

## Example

```javascript
import p2panda from 'p2panda-js';

const { KeyPair } = await p2panda;
const keyPair = new KeyPair();
console.log(keyPair.publicKey());
```

## Development

For development you need the following tools:

* [Node.js](https://nodejs.org)
* [Rust](https://www.rust-lang.org/learn/get-started)
* [wasm-pack](https://rustwasm.github.io/wasm-pack/installer)

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
