// SPDX-License-Identifier: AGPL-3.0-or-later
use std::collections::HashMap;

use p2panda_rs::entry::{decode_entry, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use p2panda_rs::message::MessageEncoded;
use p2panda_tests::Panda;
use p2panda_tests::utils::MESSAGE_SCHEMA;

use serde::Serialize;
use serde_json;

#[derive(Serialize)]
pub struct EntryData{
    entryBytes: EntrySigned,
    payloadBytes: MessageEncoded,
    decoded: Entry
}

pub fn get_test_data(authors: Vec<Panda>) -> HashMap<String, HashMap<usize, Vec<EntryData>>> {
    let mut decoded_logs: HashMap<String, HashMap<usize, Vec<EntryData>>>  = HashMap::new();
    
    for author in authors {
        decoded_logs.insert(author.name(), HashMap::new());
        let author_logs = decoded_logs.get_mut(&author.name()).unwrap();
        for (log_id, entries) in author.logs.iter() {
            author_logs.insert(log_id.to_owned(), Vec::new());
            for (entry_encoded, message_encoded) in entries.iter() {
                let entry = decode_entry(entry_encoded, Some(message_encoded)).unwrap();
                let entry_data = EntryData {
                    entryBytes: entry_encoded.to_owned(),
                    payloadBytes: message_encoded.to_owned(),
                    decoded: entry
                };
                author_logs.get_mut(log_id).unwrap().push(entry_data);
            }
        }    
    }
    decoded_logs
}

fn main() {
    let mut panda = Panda::new("panda".to_string(), Panda::keypair());
    
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "hello!")]));
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "poop!")]));
    
    let (entry_encoded_1, _) = panda.get_encoded_entry_and_message(MESSAGE_SCHEMA, 1);
    panda.publish_entry(Panda::update_message(MESSAGE_SCHEMA, entry_encoded_1.hash(), vec![("message", "Smelly!")]));
    
    println!("{}", serde_json::to_string_pretty(&get_test_data(vec![panda])).unwrap());
}
