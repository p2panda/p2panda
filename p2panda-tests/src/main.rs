// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json;

use p2panda_rs::tests::utils::{
    create_message, delete_message, fields, new_key_pair, update_message, MESSAGE_SCHEMA,
};
use p2panda_tests::utils::to_test_data;
use p2panda_tests::Panda;

fn main() {
    // Create an author named "panda"
    let mut panda = Panda::new("panda".to_string(), new_key_pair());

    // Publish an entry to their log
    let entry_encoded_1 = panda.publish_entry(create_message(
        MESSAGE_SCHEMA.into(),
        fields(vec![("message", "Ohh, my first message!")]),
    ));

    // Update the instance created by the first published entry
    panda.publish_entry(update_message(
        MESSAGE_SCHEMA.into(),
        entry_encoded_1.hash(),
        fields(vec![("message", "Which I now update.")]),
    ));

    // Delete the instance
    panda.publish_entry(delete_message(
        MESSAGE_SCHEMA.into(),
        entry_encoded_1.hash(),
    ));

    // Publish a new message
    panda.publish_entry(create_message(
        MESSAGE_SCHEMA.into(),
        fields(vec![("message", "Let's try that again.")]),
    ));

    // Format the log data contained by this author
    let formatted_data = to_test_data(vec![panda]);

    println!("{}", serde_json::to_string_pretty(&formatted_data).unwrap());
}
