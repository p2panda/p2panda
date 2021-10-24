
use std::convert::TryInto;
use std::collections::HashMap;
use serde::Serialize;
use bamboo_rs_core::entry::is_lipmaa_required;

use p2panda_rs::entry::{decode_entry, EntrySigned, LogId, SeqNum};
use p2panda_rs::message::MessageEncoded;
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::Author;
use p2panda_rs::message::Message;

use crate::Panda;

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


pub fn calculate_entry_args(
    log_id: usize,
    schema_entries: Vec<(EntrySigned, MessageEncoded)>,
) -> NextEntryArgs {
    if schema_entries.len() == 0 {
        NextEntryArgs {
            entryHashBacklink: None,
            entryHashSkiplink: None,
            seqNum: SeqNum::new(1).unwrap(),
            logId: LogId::new(log_id.try_into().unwrap()),
        }
    } else {
        // Get last entry in log
        let (entry_encoded, _) = schema_entries.get(schema_entries.len() - 1).unwrap();
        let decoded_last_entry = decode_entry(&entry_encoded, None).unwrap();

        // Get the hash (which is the backlink we need)
        let backlink = Some(entry_encoded.hash());

        // Get the next sequence number
        let next_seq_num = decoded_last_entry.seq_num().to_owned().next().unwrap();

        // And then the skiplink
        let skiplink_seq_num = next_seq_num.skiplink_seq_num().unwrap().as_i64();

        // And finally the log id
        let log_id = decoded_last_entry.log_id();

        // Check if skiplink is required and return hash if so
        let skiplink = if is_lipmaa_required(next_seq_num.as_i64() as u64) {
            let (skiplink_entry, _) = schema_entries.get(skiplink_seq_num as usize).unwrap();
            Some(skiplink_entry.hash())
        } else {
            None
        };

        NextEntryArgs {
            entryHashBacklink: backlink,
            entryHashSkiplink: skiplink,
            seqNum: next_seq_num,
            logId: log_id.to_owned(),
        }
    }
}

/// Convert log data from a vector of authors into structs which can be json formatted
/// how we would like for our tests.
pub fn to_test_data(authors: Vec<Panda>) -> HashMap<String, AuthorData> {
    let mut decoded_logs: HashMap<String, AuthorData> = HashMap::new();

    for author in authors {
        let mut author_logs = Vec::new();
        for (log_id, entries) in author.logs.iter() {
            let mut log_data = LogData {
                encodedEntries: Vec::new(),
                decodedMessages: Vec::new(),
                nextEntryArgs: Vec::new(),
            };

            for (entry_encoded, message_encoded) in entries.iter() {
                let entry = decode_entry(entry_encoded, Some(message_encoded)).unwrap();
                let next_entry_args =
                    author.next_entry_args_for_specific_entry(log_id.to_owned(), entry.seq_num());
                let message_decoded = entry.message().unwrap();
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
                log_data.nextEntryArgs.push(next_entry_args);
            }
            let final_next_entry_args = author.next_entry_args(log_id.to_owned());
            log_data.nextEntryArgs.push(final_next_entry_args);
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
