<h1 align="center">p2panda-rs test_utils</h1>

<div align="center">
  <strong>All the things a panda needs (for testing)</strong>
</div>

<br />

`test_utils` provides tools for [p2panda](https://github.com/p2panda/p2panda) which can be used for testing in Rust and the generation of test data.

## Features

- Fixtures
- Fixture templates
- A mock node
- A mock client
- Methods for generating test data

## Test data

### Generate

```bash
# Generate JSON formatted test data
cargo run --bin json-test-data
```

### Format

Test data is generated as a JSON document formatted as summerised below (to see full output, run command mentioned above).

```js
{
  // Arbitrary name for identifying author in tests
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
        "decodedOperations": [
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
            "action": "update",
            //...
          },
          {
            "action": "delete",
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

### Usage in `p2panda-js`

The generated test data is used to validate JSON RPC requests in `p2panda-js`. See `generate_test_data.rs` to find out how the test data is formed.
