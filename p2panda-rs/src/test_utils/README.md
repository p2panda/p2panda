<h1 align="center">p2panda-rs test_utils</h1>

<div align="center">
  <strong>All the things a panda needs (for testing)</strong>
</div>

<br />

`test_utils` provides tools for [p2panda](https://github.com/p2panda/p2panda) which can be used for testing in Rust and the generation of test data.

## Parameter Fixtures

These fixtures can be injected into tests via there parameters. They can be customised using the `#[case]` marco otherwise default values will be used.
A simple example is shown below, see module level [docs](https://github.com/p2panda/p2panda/blob/main/p2panda-rs/src/test_utils/fixtures/mod.rs) for more examples.

```rs
use rstest::rstest;
use crate::entry::Entry;
use crate::identity::KeyPair;
use crate::test_utils::fixtures::{entry, key_pair};

// In this test `entry` and `key_pair` are injected directly into the test, they were imported from
// the fixtures module above and their default values will be used.
#[cfg(test)]
fn encode_entry(entry: Entry, key_pair: KeyPair) {
  assert!(sign_and_encode(&entry, &key_pair).is_ok());
}


```

## Fixture templates

These can be used to apply a collection of different parameter fixtures to a single test.

```rs
use rstest::rstest;
use crate::entry::Entry;
use crate::identity::KeyPair;
use crate::test_utils::fixtures::{entry, key_pair};
use crate::test_utils::fixtures::templates::many_valid_entries;
// This test is similar to the first, but now using a template we can test against many different
// valid entries.
#[apply(many_valid_entries)]
fn encode_multiple_entries(#[case] entry: Entry, key_pair: KeyPair) {
    assert!(sign_and_encode(&entry, &key_pair).is_ok());
}
```

## Mock Node & Client

The two structs `Node` and `Client` can be used to simulate networking traffic with multiple authors that would exist in a live p2panda network. These
are to be used in testing when needing to generate common p2panda data which relies on communication between these different parts.

```rs
// IMPORTS MISSING FOR BREVITY //
// We are using many tools from the `test_utils` in this example, please see module level docs for more thorough
// examples.

let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
let penguin = Client::new("penguin".to_string(), KeyPair::new());

let mut node = Node::new();

// Panda publishes a create operation. In doing so they create a document log which contains one entry.
//
// PANDA  : [1]
// PENGUIN:
let (panda_entry_1_hash, next_entry_args) = send_to_node(
    &mut node,
    &panda,
    &create_operation(
        hash(DEFAULT_SCHEMA_HASH),
        operation_fields(vec![(
            "cafe_name",
            OperationValue::Text("Panda Cafe".to_string()),
        )]),
    ),
)
.unwrap();

// Panda publishes an update operation. This appends a new entry to the document log, the operation also refers to the previous
// tip of the graph by it's hash id.
//
// PANDA  : [1] <--[2]
// PENGUIN:
let (panda_entry_2_hash, next_entry_args) = send_to_node(
    &mut node,
    &panda,
    &update_operation(
        hash(DEFAULT_SCHEMA_HASH),
        vec![panda_entry_1_hash.clone()], // The previous tip of this document graph
        operation_fields(vec![(
            "cafe_name",
            OperationValue::Text("Panda Cafe!!".to_string()),
        )]),
    ),
)
.unwrap();

// Now Penguin publishes an update operation which refers to Panda's document (via it's tip operation). In doing this
// Penguin creates their own document log which now contains one entry, the operation on this entry refers to Panda's document
// log tip.
//
// PANDA  : [1] <--[2]
// PENGUIN:           \--[1]
let (penguin_entry_1_hash, next_entry_args) = send_to_node(
    &mut node,
    &penguin,
    &update_operation(
        hash(DEFAULT_SCHEMA_HASH),
        vec![panda_entry_2_hash],
        operation_fields(vec![(
            "message",
            OperationValue::Text("Penguin Cafe".to_string()),
        )]),
    ),
)
.unwrap();
```

## Logging

To enable logging in the mock node run the test suite with the following env vars set:

```bash
# Run tests with info logging
RUST_LOG=p2panda_rs=info cargo test

# Run tests with info and debug logging
RUST_LOG=p2panda_rs=debug cargo test
```

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
