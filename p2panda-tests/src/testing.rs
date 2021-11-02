use serde::Serialize;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};

use p2panda_rs::entry::{decode_entry, LogId, SeqNum};
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::Author;
use p2panda_rs::message::{Message, MessageEncoded};
use p2panda_rs::tests::utils::MESSAGE_SCHEMA;

use crate::node::Node;
use crate::client::Client;

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct NextEntryArgs {
    pub entryHashBacklink: Option<Hash>,
    pub entryHashSkiplink: Option<Hash>,
    pub seqNum: SeqNum,
    pub logId: LogId,
}

/// Structs for formatting our author log data into what we want for tests
#[derive(Serialize)]
#[allow(non_snake_case)]
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
#[allow(non_snake_case)]
pub struct LogData {
    encodedEntries: Vec<EncodedEntryData>,
    decodedMessages: Vec<Message>,
    nextEntryArgs: Vec<NextEntryArgs>,
}

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct AuthorData {
    publicKey: String,
    privateKey: String,
    logs: Vec<LogData>,
}

/// Convert log data from a vector of authors into structs which can be json formatted
/// how we would like for our tests.
pub fn generate_test_data(node: &mut Node, clients: Vec<Client>) -> HashMap<String, AuthorData> {
    let mut decoded_logs: HashMap<String, AuthorData> = HashMap::new();

    for (author_hash, author_logs) in node.db() {
        let author = Author::new(&author_hash).unwrap();
        let mut author_logs_data = Vec::new();
        for (_log_id, log) in author_logs.iter() {
            let mut log_data = LogData {
                encodedEntries: Vec::new(),
                decodedMessages: Vec::new(),
                nextEntryArgs: Vec::new(),
            };

            for log_entry in log.entries().iter() {
                let message_encoded = MessageEncoded::try_from(&log_entry.message).unwrap();
                let entry = decode_entry(&log_entry.entry_encoded, Some(&message_encoded)).unwrap();
                let next_entry_args = node
                    .next_entry_args_for_specific_entry(
                        &author,
                        &Hash::new(MESSAGE_SCHEMA).unwrap(),
                        entry.seq_num(),
                    )
                    .unwrap();
                let message_decoded = entry.message().unwrap();
                let entry_data = EncodedEntryData {
                    author: author.clone(),
                    entryBytes: log_entry.entry_encoded.as_str().into(),
                    entryHash: log_entry.entry_encoded.hash(),
                    payloadBytes: message_encoded.as_str().into(),
                    payloadHash: message_encoded.hash(),
                    logId: entry.log_id().to_owned(),
                    seqNum: entry.seq_num().to_owned(),
                };

                log_data.encodedEntries.push(entry_data);
                log_data.decodedMessages.push(message_decoded.to_owned());
                
                // Ugly hack for converting keys into what we expect in JS testing world
                let json_entry_args = NextEntryArgs {
                    entryHashBacklink: next_entry_args.backlink,
                    entryHashSkiplink: next_entry_args.skiplink,
                    seqNum: next_entry_args.seq_num,
                    logId: next_entry_args.log_id,
                };

                log_data.nextEntryArgs.push(json_entry_args);
            }
            let final_next_entry_args = node.next_entry_args(&author, &Hash::new(MESSAGE_SCHEMA).unwrap()).unwrap();

            // Ugly hack for converting keys into what we expect in JS testing world
            let json_entry_args = NextEntryArgs {
                entryHashBacklink: final_next_entry_args.backlink,
                entryHashSkiplink: final_next_entry_args.skiplink,
                seqNum: final_next_entry_args.seq_num,
                logId: final_next_entry_args.log_id,
            };

            log_data.nextEntryArgs.push(json_entry_args);
            author_logs_data.push(log_data);
        }    
            
        let client = clients.iter().find(|client| client.public_key() == author.as_str()).unwrap();
        
        let author_data = AuthorData {
            publicKey: client.public_key(),
            privateKey: client.private_key(),
            logs: author_logs_data,
        };
        decoded_logs.insert(client.name(), author_data);
    }
    decoded_logs
}
