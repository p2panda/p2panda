// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json;

use p2panda_rs::tests::utils::{
    create_message, delete_message, fields, new_key_pair, update_message, MESSAGE_SCHEMA,
};
use p2panda_tests::data::generate_test_data;
use p2panda_tests::client::Client;
use p2panda_tests::node::Node;
use p2panda_tests::utils::send_to_node;

fn main() {
    let mut node = Node::new();

    let panda = Client::new("panda".to_string(), new_key_pair());

    let instance_a_hash = send_to_node(&mut node, &panda, &create_message(
        MESSAGE_SCHEMA.into(),
        fields(vec![("message", "Ohh, my first message!")]),
    )).unwrap();

    send_to_node(&mut node, &panda, &update_message(
        MESSAGE_SCHEMA.into(),
        instance_a_hash.clone(),
        fields(vec![("message", "Which I now update.")]),
    )).unwrap();
    
    send_to_node(&mut node, &panda, &delete_message(
        MESSAGE_SCHEMA.into(),
        instance_a_hash,
    )).unwrap();

    send_to_node(&mut node, &panda, &create_message(
        MESSAGE_SCHEMA.into(),
        fields(vec![("message", "Let's try that again.")]),
    )).unwrap();
    
    let db = node.db();
    let entries = node.all_entries();
    let query = node.query_all(&MESSAGE_SCHEMA.to_string()).unwrap();
    
    println!("{:#?}", db);
    println!("{:#?}", entries);
    println!("{:#?}", query);
    
    let formatted_data = generate_test_data(&mut node, vec![panda]);
    
    println!("{}", serde_json::to_string_pretty(&formatted_data).unwrap());
}
