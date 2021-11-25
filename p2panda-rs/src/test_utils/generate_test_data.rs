// SPDX-License-Identifier: AGPL-3.0-or-later

/// Generate json formatted test data
use serde_json;

use p2panda_rs::test_utils::client::Client;
use p2panda_rs::test_utils::data::generate_test_data;
use p2panda_rs::test_utils::node::send_to_node;
use p2panda_rs::test_utils::node::Node;
use p2panda_rs::test_utils::{
    create_message, delete_message, hash, message_fields, new_key_pair, update_message,
    DEFAULT_SCHEMA_HASH,
};

fn main() {
    // Instanciate mock node
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

    use p2panda_rs::test_utils::client::Client;
    use p2panda_rs::test_utils::data::generate_test_data;
    use p2panda_rs::test_utils::node::send_to_node;
    use p2panda_rs::test_utils::node::Node;
    use p2panda_rs::test_utils::{
        create_message, hash, keypair_from_private, message_fields, DEFAULT_PRIVATE_KEY,
        DEFAULT_SCHEMA_HASH,
    };

    #[test]
    fn test_data() {
        // Instanciate mock node
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
                                "entryBytes": "002f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc960101dc00402d22e064b16c17209a0a60e6326904effa2a34ad442fe9c8d86664a70b313a464b4dca3aa0927a299968e82d79f29e100191b3f32d8eda63b340fb9757b670609ade5b150be29f2334afe86b277153dd587c5fcea2dd82812df0d9fb5db234efe51883e0234242beae89d439a05947e1530d2282dad74e91d2e5915a08929c0c",
                                "entryHash": "0040f80486aa52d765acaa2d1e349a1ab62d6f3ae254af263da4e94ab746a8a9aa08ec7c81f18fe563faaae05927bc13815aec4c08b23836f2c0504d9bb356f8f4c6",
                                "payloadBytes": "a466616374696f6e6663726561746566736368656d6178843030343031643736353636373538613562366266633536316631633933366438666338366235623432656132326162316461626634306432343964323764643930363430316664653134376535336634346331303364643032613235343931366265313133653531646531303737613934366133613063313237326239623334383433376776657273696f6e01666669656c6473a1676d657373616765a26474797065637374726576616c7565764f68682c206d79206669727374206d65737361676521",
                                "payloadHash": "00402d22e064b16c17209a0a60e6326904effa2a34ad442fe9c8d86664a70b313a464b4dca3aa0927a299968e82d79f29e100191b3f32d8eda63b340fb9757b67060",
                                "logId": 1,
                                "seqNum": 1
                            }
                        ],
                        "decodedMessages": [
                            {
                                "action": "create",
                                "schema": "00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437",
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
                                "seqNum": 1,
                                "logId": 1
                            },
                            {
                                "entryHashBacklink": "0040f80486aa52d765acaa2d1e349a1ab62d6f3ae254af263da4e94ab746a8a9aa08ec7c81f18fe563faaae05927bc13815aec4c08b23836f2c0504d9bb356f8f4c6",
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
