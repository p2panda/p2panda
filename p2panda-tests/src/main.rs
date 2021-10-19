// SPDX-License-Identifier: AGPL-3.0-or-later
use std::collections::HashMap;

use p2panda_rs::entry::{decode_entry, LogId, SeqNum};
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::Author;
use p2panda_rs::message::Message;
use p2panda_tests::utils::MESSAGE_SCHEMA;
use p2panda_tests::{NextEntryArgs, Panda};

use serde::Serialize;
use serde_json;

/// Structs for formatting our author log data into what we want for tests
#[derive(Serialize)]
pub struct EncodedEntryData {
    author: Author,
    entryBytes: String,
    entryHash: Hash,
    payloadBytes: String,
    payloadHash: Hash,
    logId: LogId,
    seqNum: SeqNum,
}

#[derive(Serialize)]
pub struct LogData {
    encodedEntries: Vec<EncodedEntryData>,
    decodedMessages: Vec<Message>,
    nextEntryArgs: NextEntryArgs
}

#[derive(Serialize)]
pub struct AuthorData {
    publicKey: String,
    privateKey: String,
    logs: Vec<LogData>,
}

/// Convert log data from a vector of authors into structs which can be json formatted
/// how we would like for our tests
pub fn get_test_data(authors: Vec<Panda>) -> HashMap<String, AuthorData> {
    let mut decoded_logs: HashMap<String, AuthorData> = HashMap::new();

    for author in authors {
        let mut author_logs = Vec::new();
        for (log_id, entries) in author.logs.iter() {
            
            let entry_args = author.next_entry_args(log_id.to_owned());

            let mut log_data = LogData {
                encodedEntries: Vec::new(),
                decodedMessages: Vec::new(),
                nextEntryArgs: entry_args,
            };

            for (entry_encoded, message_encoded) in entries.iter() {
                let entry = decode_entry(entry_encoded, Some(message_encoded)).unwrap();
                let message_decoded = entry.message().unwrap();
                // EncodedEntryData
                let entry_data = EncodedEntryData {
                    author: entry_encoded.author(),
                    entryBytes: entry_encoded.as_str().into(),
                    entryHash: entry_encoded.hash(),
                    payloadBytes: message_encoded.as_str().into(),
                    payloadHash: message_encoded.hash(),
                    logId: entry.log_id().to_owned(),
                    seqNum: entry.seq_num().to_owned(),
                };

                log_data.encodedEntries.push(entry_data);
                log_data.decodedMessages.push(message_decoded.to_owned());
            }
            author_logs.push(log_data);
        }

        let author_data = AuthorData {
            publicKey: author.public_key(),
            privateKey: author.private_key(),
            logs: author_logs,
        };
        decoded_logs.insert(author.name(), author_data);
    }
    decoded_logs
}

fn main() {
    // Create an author named "panda"
    let mut panda = Panda::new("panda".to_string(), Panda::keypair());

    // Publish an entry to their log
    panda.publish_entry(Panda::create_message(
        MESSAGE_SCHEMA,
        vec![("message", "One create message.")],
    ));

    // Publish some more entries
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "Two create message.")]));
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "Three create message.")]));
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "Four!")]));

    // Create an author named "panda"
    let mut penguin = Panda::new("penguin".to_string(), Panda::keypair());

    // Publish an entry to their log
    penguin.publish_entry(Panda::create_message(
        MESSAGE_SCHEMA,
        vec![("message", "Now I will read a poem.")],
    ));

    // Publish some more entries
    penguin.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "Ahh, I'm too nervous.")]));
    penguin.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "Let me try that again.")]));

    let (entry_encoded_1, _) = penguin.get_encoded_entry_and_message(MESSAGE_SCHEMA, 1);
    penguin.publish_entry(Panda::update_message(MESSAGE_SCHEMA, entry_encoded_1.hash(), vec![("message", "Now I will buy an ice coffee.")]));

    // Format the log data contained by this author
    let formatted_data = get_test_data(vec![panda, penguin]);
    
    println!("{}", serde_json::to_string_pretty(&formatted_data).unwrap());
}
