<h1 align="center">p2panda-rs</h1>

<div align="center">
  <strong>All the things a panda needs (for Rust)</strong>
</div>

<br/>

## Example

```rust
use p2panda_rs::KeyPair;

let key_pair = KeyPair::new();
println!("{}", key_pair.publicKey());
```

## Development

You will need the following tools to start development:

* [Rust](https://www.rust-lang.org/learn/get-started)
* [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

```bash
# Run tests
cargo test

# Compile wasm as npm browser package into `pkg` folder
wasm-pack build
```

## License

GNU Affero General Public License v3.0 `AGPL-3.0`
