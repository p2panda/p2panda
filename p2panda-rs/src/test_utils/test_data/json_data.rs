// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structs and methods needed for generating test data in json format.
//!
//! This is used for `p2panda-js` tests.
use std::collections::HashMap;
use std::convert::TryFrom;

use serde::Serialize;

use crate::entry::{decode_entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{Operation, OperationEncoded};
use crate::test_utils::mocks::Client;
use crate::test_utils::mocks::Node;

/// Next entry args formatted correctly for the test data.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NextEntryArgs {
    entry_hash_backlink: Option<Hash>,
    entry_hash_skiplink: Option<Hash>,
    seq_num: SeqNum,
    log_id: LogId,
}

/// Encoded entry data formatted correctly for the test data.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EncodedEntryData {
    author: Author,
    entry_bytes: String,
    entry_hash: Hash,
    payload_bytes: String,
    payload_hash: Hash,
    log_id: LogId,
    seq_num: SeqNum,
}

/// Log data formatted correctly for the test data.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LogData {
    encoded_entries: Vec<EncodedEntryData>,
    decoded_operations: Vec<Operation>,
    next_entry_args: Vec<NextEntryArgs>,
}

/// Author data formatted correctly for the test data.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AuthorData {
    public_key: String,
    private_key: String,
    logs: Vec<LogData>,
}

/// Convert log data from a vector of authors into structs which can be json formatted in the way
/// we expect for our test data.
pub fn generate_test_data(node: &mut Node, clients: Vec<Client>) -> HashMap<String, AuthorData> {
    // Init test data map
    let mut test_data: HashMap<String, AuthorData> = HashMap::new();

    // Iterate over authors
    for (author_hash, author_logs) in node.db() {
        let author = Author::new(&author_hash).unwrap();
        let mut author_logs_data = Vec::new();

        // Iterate over the authors logs
        for log in author_logs.iter() {
            // Init empty log data
            let mut log_data = LogData {
                encoded_entries: Vec::new(),
                decoded_operations: Vec::new(),
                next_entry_args: Vec::new(),
            };

            // Set the document id
            let document_id = log.entries()[0].hash();

            // Iterate over entries in this document log
            for log_entry in log.entries().iter() {
                // Encode the operation
                let operation_encoded = OperationEncoded::try_from(&log_entry.operation()).unwrap();

                // Decode the entry adding it's operation back in
                let entry =
                    decode_entry(&log_entry.entry_encoded, Some(&operation_encoded)).unwrap();

                // Compose encoded entry data
                let entry_data = EncodedEntryData {
                    author: author.clone(),
                    entry_bytes: log_entry.entry_encoded().as_str().into(),
                    entry_hash: log_entry.entry_encoded().hash(),
                    payload_bytes: operation_encoded.as_str().into(),
                    payload_hash: operation_encoded.hash(),
                    log_id: entry.log_id().to_owned(),
                    seq_num: entry.seq_num().to_owned(),
                };

                // Get next entry args for this document log
                let next_entry_args = node
                    .get_next_entry_args(&author, Some(&document_id), Some(entry.seq_num()))
                    .unwrap();

                let json_entry_args = NextEntryArgs {
                    entry_hash_backlink: next_entry_args.backlink,
                    entry_hash_skiplink: next_entry_args.skiplink,
                    seq_num: next_entry_args.seq_num,
                    log_id: next_entry_args.log_id,
                };

                // Push all data for this entry to the log
                log_data.encoded_entries.push(entry_data);
                log_data.decoded_operations.push(log_entry.operation());
                log_data.next_entry_args.push(json_entry_args);
            }

            // Get the final next entry args for this log
            let final_next_entry_args = node
                .get_next_entry_args(&author, Some(&log.document()), None)
                .unwrap();

            let json_entry_args = NextEntryArgs {
                entry_hash_backlink: final_next_entry_args.backlink,
                entry_hash_skiplink: final_next_entry_args.skiplink,
                seq_num: final_next_entry_args.seq_num,
                log_id: final_next_entry_args.log_id,
            };

            // Push next entry args to the log data
            log_data.next_entry_args.push(json_entry_args);

            // Push the log data to the author log array
            author_logs_data.push(log_data);
        }

        // Get the client name
        let client = clients
            .iter()
            .find(|client| client.public_key() == author.as_str())
            .unwrap();

        // Compose the author data
        let author_data = AuthorData {
            public_key: client.public_key(),
            private_key: client.private_key(),
            logs: author_logs_data,
        };

        // Push all data for this author to the test data
        test_data.insert(client.name(), author_data);
    }
    test_data
}
