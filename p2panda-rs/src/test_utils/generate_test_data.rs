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
