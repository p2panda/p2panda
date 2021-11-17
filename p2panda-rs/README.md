<h1 align="center">p2panda-rs</h1>

<div align="center">
  <strong>All the things a panda needs (for Rust)</strong>
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
    <a href="https://docs.rs/p2panda-rs">
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
  </h3>
</div>

<br />

This library provides all tools required to write a client for the [`p2panda`] network. It is shipped both as a Rust crate [`p2panda-rs`] with WebAssembly bindings and a NPM package [`p2panda-js`] with TypeScript definitions running in NodeJS or any modern web browser.

Read the library [`documentation`] for installation guides and examples.

[`documentation`]: https://github.com/p2panda/p2panda
[`p2panda-js`]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
[`p2panda-rs`]: https://github.com/p2panda/p2panda/tree/main/p2panda-rs
[`p2panda`]: https://github.com/p2panda/handbook

## Development

You will need the following tools to start development:

- [Rust](https://www.rust-lang.org/learn/get-started)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)

```bash
# Run tests
cargo test

# Compile wasm as npm browser package into `pkg` folder
wasm-pack build
```

## Test Utils

`src/test_utils` provides tools which can be used for testing in `rust` and the generation of test data in `JSON` format. 

It includes:
- fixtures
- fixture templates
- a mock node
- a mock client
- methods for generating test data

## Test Data

Test data is generated as a json document formatted as summerised below (to see full output, run `cargo run`).

```js
{
  // arbitrary name for identifying author in tests
  "panda": {
    "publicKey": "...",
    "privateKey": "...",
    "logs": [
      {
        "encodedEntries": [
          {
            "author": "...",
            "entryBytes": "...",
            "entryHash": "...",
            "payloadBytes": "...",
            "payloadHash": "...",
            "logId": 1,
            "seqNum": 1
          },
          //...
          {
            "author": "...",
            //...
            "seqNum": 4
          }
        ],
        "decodedMessages": [
          {
            "action": "create",
            "schema": "...",
            "version": 1,
            "fields": {
              "message": {
                "type": "str",
                "value": "..."
              }
            }
          },
          {
            "action": "update",
            //...
          },
          {
            "action": "delete",
            //...
          },
          {
            "action": "create",
            //...
          }
        ],
        "nextEntryArgs": [
          {
            "entryHashBacklink": null,
            "entryHashSkiplink": null,
            "seqNum": 1,
            "logId": 1
          },
          //...
          {
            "entryHashBacklink": "...",
            "entryHashSkiplink": null,
            "seqNum": 5,
            "logId": 1
          }
        ]
      }
    ]
  }
}
```

## License

GNU Affero General Public License v3.0 [`AGPL-3.0-or-later`](LICENSE)

## Supported by

<img src="https://p2panda.org/images/ngi-logo.png" width="auto" height="80px"><br /><img src="https://p2panda.org/images/eu-flag-logo.png" width="auto" height="80px">

*This project has received funding from the European Union’s Horizon 2020 research and innovation programme within the framework of the NGI-POINTER Project funded under grant agreement No 871528*
