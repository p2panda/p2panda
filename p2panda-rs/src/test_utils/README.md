<h1 align="center">p2panda-rs test utils</h1>

<div align="center">
  <strong>All the things a panda needs (for testing)</strong>
</div>

<br />

This module provides tools used for testing and generating test data for `p2panda-rs` and `p2panda-js`. 

It includes:
- a mock node
- a mock client
- methods for generating test data (used in `p2panda-js` tests)

## Development

You will need the following tools to start development:

- [Rust](https://www.rust-lang.org/learn/get-started)

```bash
# Run tests
cargo test

# Generate test data json output (from `main.rs`)
cargo run

```

## Test Data

Test data is generated as a json document formatted as summerised below (to see full output, run `cargo run`). Currently in the `p2panda-js` tests we need the data to consist of a single author with a single log which contains 4 entries published with the following message types in this exact order -> 1: CREATE, 2: UPDATE, 3: DELETE and 4: CREATE.

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
