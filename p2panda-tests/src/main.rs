// SPDX-License-Identifier: AGPL-3.0-or-later
use std::collections::HashMap;

use p2panda_rs::entry::{decode_entry, LogId, SeqNum};
use p2panda_rs::identity::Author;
use p2panda_rs::hash::Hash;
use p2panda_rs::message::Message;
use p2panda_tests::{Panda, NextEntryArgs};
use p2panda_tests::utils::MESSAGE_SCHEMA;

use serde::Serialize;
use serde_json;

#[derive(Serialize)]
pub struct EncodedEntryData {
    author: Author,
    entryBytes: String,
    entryHash: Hash,
    payloadBytes: String,
    payloadHash: Hash,
    logId: LogId,
    seqNum: SeqNum
}

#[derive(Serialize)]
pub struct LogData{
    encodedEntries: Vec<EncodedEntryData>,
    decodedMessages: Vec<Message>,
    nextEntryArgs: NextEntryArgs
}

#[derive(Serialize)]
pub struct AuthorData {
    publicKey: String,
    privateKey: String,
    logs: Vec<LogData>
}

pub fn get_test_data(authors: Vec<Panda>) -> HashMap<String, AuthorData> {
    let mut decoded_logs: HashMap<String, AuthorData>  = HashMap::new();
    
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
        

        let author_data = AuthorData { publicKey: author.public_key(), privateKey: author.private_key(), logs: author_logs };
        decoded_logs.insert(author.name(), author_data);
    }
    decoded_logs
}

fn main() {
    let mut panda = Panda::new("panda".to_string(), Panda::keypair());
    
    panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "hello!")]));
    // panda.publish_entry(Panda::create_message(MESSAGE_SCHEMA, vec![("message", "poop!")]));
    
    // let (entry_encoded_1, _) = panda.get_encoded_entry_and_message(MESSAGE_SCHEMA, 1);
    // panda.publish_entry(Panda::update_message(MESSAGE_SCHEMA, entry_encoded_1.hash(), vec![("message", "Smelly!")]));
    
    println!("{}", serde_json::to_string_pretty(&get_test_data(vec![panda])).unwrap());
}
