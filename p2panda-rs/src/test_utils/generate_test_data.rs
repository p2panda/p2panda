// SPDX-License-Identifier: AGPL-3.0-or-later

/// Generate JSON formatted test data. This is run with the `cargo run --bin json-test-data`
/// command. The output data can be used for testing a p2panda implementation. It is currently used
/// in `p2panda-js`.
use p2panda_rs::operation::OperationValue;
use p2panda_rs::schema::SchemaId;
use p2panda_rs::test_utils::constants::DEFAULT_SCHEMA_HASH;
use p2panda_rs::test_utils::mocks::Client;
use p2panda_rs::test_utils::mocks::{send_to_node, Node};
use p2panda_rs::test_utils::test_data::json_data::generate_test_data;
use p2panda_rs::test_utils::utils::{
    create_operation, delete_operation, new_key_pair, operation_fields, update_operation,
};

fn main() {
    // Instantiate mock node
    let mut node = Node::new();

    // Instantiate one client called "panda"
    let panda = Client::new("panda".to_string(), new_key_pair());

    let schema_id = SchemaId::new(&format!("venue_{}", DEFAULT_SCHEMA_HASH)).unwrap();

    // Publish a CREATE operation
    let (entry1_hash, _) = send_to_node(
        &mut node,
        &panda,
        &create_operation(
            schema_id.clone(),
            operation_fields(vec![(
                "message",
                OperationValue::Text("Ohh, my first message!".to_string()),
            )]),
        ),
    )
    .unwrap();

    // Publish an UPDATE operation
    let (entry2_hash, _) = send_to_node(
        &mut node,
        &panda,
        &update_operation(
            schema_id.clone(),
            vec![entry1_hash.into()],
            operation_fields(vec![(
                "message",
                OperationValue::Text("Which I now update.".to_string()),
            )]),
        ),
    )
    .unwrap();

    // Publish another UPDATE operation
    let (entry3_hash, _) = send_to_node(
        &mut node,
        &panda,
        &update_operation(
            schema_id.clone(),
            vec![entry2_hash.into()],
            operation_fields(vec![(
                "message",
                OperationValue::Text("And then update again.".to_string()),
            )]),
        ),
    )
    .unwrap();

    // Publish an DELETE operation
    send_to_node(
        &mut node,
        &panda,
        &delete_operation(schema_id, vec![entry3_hash.into()]),
    )
    .unwrap();

    // Get the database represented as json and formatted ready to be used as test data in
    // `p2panda-js`
    let formatted_data = generate_test_data(&mut node, vec![panda]);

    println!("{}", serde_json::to_string_pretty(&formatted_data).unwrap());
}

#[cfg(test)]
mod tests {
    use p2panda_rs::schema::SchemaId;
    // Generate json formatted test data
    use serde_json::Value;

    use p2panda_rs::operation::{OperationId, OperationValue};
    use p2panda_rs::test_utils::constants::{DEFAULT_PRIVATE_KEY, DEFAULT_SCHEMA_HASH};
    use p2panda_rs::test_utils::mocks::Client;
    use p2panda_rs::test_utils::mocks::{send_to_node, Node};
    use p2panda_rs::test_utils::test_data::json_data::generate_test_data;
    use p2panda_rs::test_utils::utils::{create_operation, keypair_from_private, operation_fields};

    #[test]
    fn test_data() {
        // Instantiate mock node
        let mut node = Node::new();

        // Instantiate one client called "panda"
        let panda = Client::new(
            "panda".to_string(),
            keypair_from_private(DEFAULT_PRIVATE_KEY.into()),
        );

        // Publish a CREATE operation
        send_to_node(
            &mut node,
            &panda,
            &create_operation(
                SchemaId::new_application(
                    "chat",
                    &DEFAULT_SCHEMA_HASH.parse::<OperationId>().unwrap().into(),
                ),
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Ohh, my first message!".to_string()),
                )]),
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
                      "entryBytes": "002f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc960101a10020ef77234ca495937e15768e4226edd487128bcc43c9ea0cd390846c40864d5300035a2bb1bc87e679415a3fd1b71ee765db1c63da2eecb15032f092a658a912bdd7a308f2e3027b15d4018080d14a12a8eadaa3f13032e06b2a27295bd0b79509",
                      "entryHash": "002065f74f6fd81eb1bae19eb0d8dce145faa6a56d7b4076d7fba4385410609b2bae",
                      "payloadBytes": "a466616374696f6e6663726561746566736368656d617849636861745f30303230633635353637616533376566656132393365333461396337643133663866326266323364626463336235633762396162343632393331313163343866633738626776657273696f6e01666669656c6473a1676d657373616765a26474797065637374726576616c7565764f68682c206d79206669727374206d65737361676521",
                      "payloadHash": "0020ef77234ca495937e15768e4226edd487128bcc43c9ea0cd390846c40864d5300",
                      "logId": "1",
                      "seqNum": "1"
                    }
                  ],
                  "decodedOperations": [
                    {
                      "action": "create",
                      "schema": "chat_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
                      "version": 1,
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
                      "seqNum": "1",
                      "logId": "1"
                    },
                    {
                      "entryHashBacklink": "002065f74f6fd81eb1bae19eb0d8dce145faa6a56d7b4076d7fba4385410609b2bae",
                      "entryHashSkiplink": null,
                      "seqNum": "2",
                      "logId": "1"
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

        // Convert both strings into json objects for comparison
        let generated_test_data_json: Value =
            serde_json::from_str(&generated_test_data_str).unwrap();
        let fixture_test_data_json: Value = serde_json::from_str(TEST_DATA).unwrap();

        // Both should be equal
        assert_eq!(generated_test_data_json, fixture_test_data_json);
    }

    #[test]
    fn test_main() {
        // Check that example values actually work
        crate::main();
    }
}
