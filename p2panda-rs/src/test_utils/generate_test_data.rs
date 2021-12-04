// SPDX-License-Identifier: AGPL-3.0-or-later

/// Generate json formatted test data. This is run with the `cargo run --bin json-test-data` command. The output
/// data can be used for testing a p2panda implementation. It is currently used in `p2panda-js`.
use serde_json;

use p2panda_rs::test_utils::mocks::client::Client;
use p2panda_rs::test_utils::mocks::node::{send_to_node, Node};
use p2panda_rs::test_utils::test_data::json_data::generate_test_data;
use p2panda_rs::test_utils::{
    create_message, delete_message, hash, message_fields, new_key_pair, update_message,
    DEFAULT_SCHEMA_HASH,
};

fn main() {
    // Instantiate mock node
    let mut node = Node::new();

    // Instantiate one client called "panda"
    let panda = Client::new("panda".to_string(), new_key_pair());

    // Publish a CREATE message
    let instance_a_hash = send_to_node(
        &mut node,
        &panda,
        &create_message(
            hash(DEFAULT_SCHEMA_HASH),
            message_fields(vec![("message", "Ohh, my first message!")]),
        ),
    )
    .unwrap();

    // Publish an UPDATE message
    send_to_node(
        &mut node,
        &panda,
        &update_message(
            hash(DEFAULT_SCHEMA_HASH),
            instance_a_hash.clone(),
            message_fields(vec![("message", "Which I now update.")]),
        ),
    )
    .unwrap();

    // Publish an DELETE message
    send_to_node(
        &mut node,
        &panda,
        &delete_message(hash(DEFAULT_SCHEMA_HASH), instance_a_hash),
    )
    .unwrap();

    // Publish another CREATE message
    send_to_node(
        &mut node,
        &panda,
        &create_message(
            hash(DEFAULT_SCHEMA_HASH),
            message_fields(vec![("message", "Let's try that again.")]),
        ),
    )
    .unwrap();

    // Get the database represented as json and formatted ready to be used as test data in `p2panda-js`
    let formatted_data = generate_test_data(&mut node, vec![panda]);

    println!("{}", serde_json::to_string_pretty(&formatted_data).unwrap());
}

#[cfg(test)]
mod tests {
    /// Generate json formatted test data
    use serde_json;
    use serde_json::Value;

    use p2panda_rs::test_utils::mocks::client::Client;
    use p2panda_rs::test_utils::mocks::node::{send_to_node, Node};
    use p2panda_rs::test_utils::test_data::json_data::generate_test_data;
    use p2panda_rs::test_utils::{
        create_message, hash, keypair_from_private, message_fields, DEFAULT_PRIVATE_KEY,
        DEFAULT_SCHEMA_HASH,
    };

    #[test]
    fn test_data() {
        // Instantiate mock node
        let mut node = Node::new();

        // Instantiate one client called "panda"
        let panda = Client::new(
            "panda".to_string(),
            keypair_from_private(DEFAULT_PRIVATE_KEY.into()),
        );

        // Publish a CREATE message
        send_to_node(
            &mut node,
            &panda,
            &create_message(
                hash(DEFAULT_SCHEMA_HASH),
                message_fields(vec![("message", "Ohh, my first message!")]),
            ),
        )
        .unwrap();

        const TEST_DATA: &str = r#"{
            "panda": {
              "publicKey": "2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96",
              "privateKey": "eb852fefa703901e42f17cdc2aa507947f392a72101b2c1a6d30023af14f75e2",
              "logs": [
                {
                  "encodedEntries": [
                    {
                      "author": "2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96",
                      "entryBytes": "002f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc9601019c002003a7c9205769a33f73fe659fe2de28cda99e2fbdb0354247a245627b0152934b7b99cc46479a6713f93a0a7b71dc126bb9b2fb55eb207b192dc0fa25dc72cfaebfcfcc7196d1524d27c30aeed63cf156471df806a930b24200035bf8d959f102",
                      "entryHash": "00200c71430f838f89fc15ea7d04dc42466c9ea8b0ba2159390d0095e1323748a258",
                      "payloadBytes": "a466616374696f6e6663726561746566736368656d61784430303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696f6e02666669656c6473a1676d657373616765a26474797065637374726576616c7565764f68682c206d79206669727374206d65737361676521",
                      "payloadHash": "002003a7c9205769a33f73fe659fe2de28cda99e2fbdb0354247a245627b0152934b",
                      "logId": 1,
                      "seqNum": 1
                    }
                  ],
                  "decodedMessages": [
                    {
                      "action": "create",
                      "schema": "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
                      "version": 2,
                      "fields": {
                        "message": {
                          "type": "str",
                          "value": "Ohh, my first message!"
                        }
                      }
                    }
                  ],
                  "nextEntryArgs": [
                    {
                      "entryHashBacklink": null,
                      "entryHashSkiplink": null,
                      "seqNum": 1,
                      "logId": 1
                    },
                    {
                      "entryHashBacklink": "00200c71430f838f89fc15ea7d04dc42466c9ea8b0ba2159390d0095e1323748a258",
                      "entryHashSkiplink": null,
                      "seqNum": 2,
                      "logId": 1
                    }
                  ]
                }
              ]
            }
          }"#;

        // Generate test data
        let generated_test_data = generate_test_data(&mut node, vec![panda]);
        // Convert to json string
        let generated_test_data_str = serde_json::to_string(&generated_test_data).unwrap();
        // Convert both strings into json objects for comparrison
        let generated_test_data_json: Value =
            serde_json::from_str(&generated_test_data_str).unwrap();
        let fixture_test_data_json: Value = serde_json::from_str(TEST_DATA).unwrap();

        // Both should be equal
        assert_eq!(generated_test_data_json, fixture_test_data_json);
    }
}
