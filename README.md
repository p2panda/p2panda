# sesamoid

Utility library for p2panda applications.

## Requirements

* [Rust](https://www.rust-lang.org/learn/get-started)
* [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

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
