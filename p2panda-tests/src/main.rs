// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json;

use p2panda_tests::utils::MESSAGE_SCHEMA;
use p2panda_tests::Panda;
use p2panda_tests::generate_test_data;

fn main() {
    // Create an author named "panda"
    let mut panda = Panda::new("panda".to_string(), Panda::keypair());

    // Publish an entry to their log
    let entry_encoded_1 = panda.publish_entry(Panda::create_message(
        MESSAGE_SCHEMA,
        vec![("message", "Ohh, my first message!")],
    ));

    // Update the instance created by the first published entry
    panda.publish_entry(Panda::update_message(MESSAGE_SCHEMA, entry_encoded_1.hash(), vec![("message", "Which I now update.")]));

    // Delete the instance
    panda.publish_entry(Panda::delete_message(MESSAGE_SCHEMA, entry_encoded_1.hash()));

    // Publish a new message
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "Let's try that again.")]));

    // Format the log data contained by this author
    let formatted_data = generate_test_data::to_test_data(vec![panda]);
    
    println!("{}", serde_json::to_string_pretty(&formatted_data).unwrap());
}
