// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json;

use p2panda_rs::tests::utils::{
    create_message, delete_message, fields, new_key_pair, update_message, MESSAGE_SCHEMA,
};
use p2panda_tests::test_data::utils::to_test_data;
use p2panda_tests::Panda;
use p2panda_tests::node::Node;
use p2panda_tests::utils::send_to_node;

fn main() {

    let mut node = Node::new();
    // Create an author named "panda"
    let panda = Panda::new("panda".to_string(), new_key_pair());

    let message = create_message(
        MESSAGE_SCHEMA.into(),
        fields(vec![("message", "Ohh, my first message!")]),
    );
    
    let instance_a_hash = send_to_node(&mut node, &panda, &message).unwrap();
        
    // Publish an entry to their log
    let update_message = update_message(
        MESSAGE_SCHEMA.into(),
        instance_a_hash,
        fields(vec![("message", "Ohh, my first message updated!")]),
    );

    send_to_node(&mut node, &panda, &update_message).unwrap();
    
    let db = node.db();
    let query = node.query_all(&MESSAGE_SCHEMA.to_string()).unwrap();
    // println!("{:#?}", db);
    // println!("{:#?}", query);
    
    // // Update the instance created by the first published entry
    // panda.publish_entry(update_message(
    //     MESSAGE_SCHEMA.into(),
    //     entry_encoded_1.hash(),
    //     fields(vec![("message", "Which I now update.")]),
    // ));

    // // Delete the instance
    // panda.publish_entry(delete_message(
    //     MESSAGE_SCHEMA.into(),
    //     entry_encoded_1.hash(),
    // ));

    // // Publish a new message
    // panda.publish_entry(create_message(
    //     MESSAGE_SCHEMA.into(),
    //     fields(vec![("message", "Let's try that again.")]),
    // ));

    // Format the log data contained by this author
    let formatted_data = to_test_data(&mut node, vec![panda]);

    println!("{}", serde_json::to_string_pretty(&formatted_data).unwrap());
}
