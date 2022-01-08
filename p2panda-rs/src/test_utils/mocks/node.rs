// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda node.
//!
//! This node mocks functionality which would be implemented in a real world p2panda node. It does
//! so in a simplistic manner and should only be used in a testing environment or demo environment.
//!
//! ## Example
//!
//! ```
//! use p2panda_rs::operation::OperationValue;
//! use p2panda_rs::test_utils::constants::DEFAULT_SCHEMA_HASH;
//! use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
//! use p2panda_rs::test_utils::utils::{
//!     create_operation, delete_operation, hash, new_key_pair, operation_fields, update_operation,
//! };
//!
//! # const CHAT_SCHEMA_HASH: &str = DEFAULT_SCHEMA_HASH;
//!
//! // Instantiate a new mock node
//! let mut node = Node::new();
//!
//! // Instantiate one client named "panda"
//! let panda = Client::new("panda".to_string(), new_key_pair());
//!
//! // Panda creates a new chat document by publishing a CREATE operation
//! let document1_hash_id = send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         operation_fields(vec![(
//!             "message",
//!             OperationValue::Text("Ohh, my first message!".to_string()),
//!         )]),
//!     ),
//!     None
//! )
//! .unwrap();
//!
//! // Panda updates the document by publishing an UPDATE operation
//! let entry2_hash = send_to_node(
//!     &mut node,
//!     &panda,
//!     &update_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         vec![document1_hash_id.clone()],
//!         operation_fields(vec![(
//!             "message",
//!             OperationValue::Text("Which I now update.".to_string()),
//!         )]),
//!     ),
//!     Some(&document1_hash_id)
//! )
//! .unwrap();
//!
//! // Panda deletes their document by publishing a DELETE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &delete_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         vec![entry2_hash]
//!     ),
//!     Some(&document1_hash_id)     
//! )
//! .unwrap();
//!
//! // Panda creates another chat document by publishing a new CREATE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         operation_fields(vec![(
//!             "message",
//!             OperationValue::Text("Let's try that again.".to_string()),
//!         )]),
//!     ),
//!     None
//! )
//! .unwrap();
//!
//! // Get all entries published to this node
//! let entries = node.all_entries();
//!
//! // There should be 4 entries
//! entries.len(); // => 4
//! ```
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
use std::collections::HashMap;
use std::convert::TryFrom;

use crate::entry::{decode_entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{Operation, OperationEncoded};
use crate::test_utils::mocks::logs::{Log, LogEntry};
use crate::test_utils::mocks::utils::Result;
use crate::test_utils::mocks::Client;
use crate::test_utils::utils::NextEntryArgs;

/// Helper method signing and encoding entry and sending it to node backend.
pub fn send_to_node(
    node: &mut Node,
    client: &Client,
    operation: &Operation,
    document_id: Option<&Hash>,
) -> Result<Hash> {
    let entry_args = node.next_entry_args(&client.author(), document_id, None)?;

    let entry_encoded = client.signed_encoded_entry(operation.to_owned(), entry_args);

    let operation_encoded = OperationEncoded::try_from(operation).unwrap();

    node.publish_entry(&entry_encoded, &operation_encoded)?;

    // Return entry hash for now so we can use it to perform UPDATE and DELETE operations later
    Ok(entry_encoded.hash())
}

/// Calculate the skiplink and backlink at a certain point in a log of entries.
fn calculate_links(seq_num: &SeqNum, log: &[LogEntry]) -> (Option<Hash>, Option<Hash>) {
    // Next skiplink hash
    let skiplink = match seq_num.skiplink_seq_num() {
        Some(seq) if is_lipmaa_required(seq_num.as_i64() as u64) => Some(
            log.get(seq.as_i64() as usize - 1)
                .expect("Skiplink missing!")
                .hash(),
        ),
        _ => None,
    };

    // Next backlink hash
    let backlink = seq_num.backlink_seq_num().map(|seq| {
        log.get(seq.as_i64() as usize - 1)
            .expect("Backlink missing!")
            .hash()
    });
    (backlink, skiplink)
}

/// Mock database type.
///
/// Maps the public key of an authors against a map of their logs identified by log id.
pub type Database = HashMap<String, HashMap<i64, Log>>;

/// This node mocks functionality which would be implemented in a real world p2panda node.
///
/// It does so in a simplistic manner and should only be used in a testing environment or demo
/// environment.
#[derive(Debug, Default)]
pub struct Node {
    /// Internal database structure.
    entries: Database,
}

impl Node {
    /// Create a new mock Node.
    pub fn new() -> Self {
        Self {
            entries: Database::new(),
        }
    }

    /// Get a mutable map of all logs published by a certain author.
    fn get_author_logs_mut(&mut self, author: &Author) -> Option<&mut HashMap<i64, Log>> {
        self.entries.get_mut(author.as_str())
    }

    /// Get a map of all logs published by a certain author.
    fn get_author_logs(&self, author: &Author) -> Option<&HashMap<i64, Log>> {
        self.entries.get(author.as_str())
    }

    /// Find the log id for the given document and author.
    pub fn get_log_id(&mut self, document_id: Option<&Hash>, author: &Author) -> Result<LogId> {
        match self.get_author_logs(author) {
            Some(logs) => {
                // Find the highest existing log id and increment it
                let next_free_log_id =
                    logs.values().map(|log| log.id().as_i64()).max().unwrap() + 1;

                match document_id {
                    Some(document) => {
                        match logs.values().find(|log| log.document() == *document) {
                            // If a log with this hash already exists, return the existing id
                            Some(log) => Ok(log.id()),
                            // Otherwise return the next free one
                            None => Ok(LogId::new(next_free_log_id)),
                        }
                    }
                    None => Ok(LogId::new(next_free_log_id)),
                }
            }
            // If there aren't any then this is the first log
            None => Ok(LogId::default()),
        }
    }

    /// Get an array of all entries in database.
    pub fn all_entries(&self) -> Vec<LogEntry> {
        let mut all_entries: Vec<LogEntry> = Vec::new();
        self.entries.iter().for_each(|(_id, author_logs)| {
            author_logs
                .iter()
                .for_each(|(_id, log)| all_entries.append(log.entries().as_mut()))
        });
        all_entries
    }

    /// Return the entire database.
    pub fn db(&self) -> Database {
        self.entries.clone()
    }

    /// Returns the log id, sequence number, skiplink and backlink hash for a given author and
    /// document. All of this information is needed to create and sign a new entry.
    ///
    /// If a value for the optional seq_num parameter is passed then next entry args *at that
    /// point* in this log are returned. This is helpful when generating test data and wanting to
    /// test the flow from requesting entry args through to publishing an entry.
    pub fn next_entry_args(
        &mut self,
        author: &Author,
        document_id: Option<&Hash>,
        seq_num: Option<&SeqNum>,
    ) -> Result<NextEntryArgs> {
        // Find out the log id for the given document and author
        let log_id = self.get_log_id(document_id, author)?;

        // Find any logs by this author for this document
        let author_log = match document_id {
            Some(document) => {
                match self.get_author_logs_mut(author) {
                    // Try to find logs of this document
                    Some(logs) => logs.values().find(|log| log.document() == *document),
                    // No logs for this author
                    None => None,
                }
            }
            // Document was not given, there is none yet!
            None => None,
        };

        // Find out the sequence number, skip- and backlink hash for the next entry in this log
        let entry_args = match author_log {
            Some(log) => {
                let mut entries = log.entries();
                let seq_num_inner = match seq_num {
                    // If a sequence number was passed ...
                    Some(s) => {
                        // ... trim the log to the point in time we are interested in
                        entries = entries[..s.as_i64() as usize - 1].to_owned();
                        // ... and return the sequence number.
                        *s
                    }
                    None => {
                        // If no sequence number was passed calculate and return the next sequence
                        // number for this log
                        SeqNum::new((log.entries().len() + 1) as i64).unwrap()
                    }
                };

                // Calculate backlink and skiplink
                let (backlink, skiplink) = calculate_links(&seq_num_inner, &entries);

                NextEntryArgs {
                    log_id,
                    seq_num: seq_num_inner,
                    skiplink,
                    backlink,
                }
            }
            // This is the first entry in the log ..
            None => NextEntryArgs {
                log_id,
                seq_num: SeqNum::new(1).unwrap(),
                skiplink: None,
                backlink: None,
            },
        };
        Ok(entry_args)
    }

    /// Store entry in the database.
    ///
    /// Please note that since this an experimental implementation this method does not validate any
    /// integrity or content of the given entry.
    pub fn publish_entry(
        &mut self,
        entry_encoded: &EntrySigned,
        operation_encoded: &OperationEncoded,
    ) -> Result<()> {
        let entry = decode_entry(entry_encoded, Some(operation_encoded))?;
        let log_id = entry.log_id().as_i64();
        let author = entry_encoded.author();

        // Get all logs by this author
        let author_logs = match self.get_author_logs_mut(&author) {
            Some(logs) => logs,
            // If there aren't any, then instantiate a new log collection
            None => {
                self.entries.insert(author.as_str().into(), HashMap::new());
                self.get_author_logs_mut(&author).unwrap()
            }
        };

        // Get the log for this document from the author logs
        let log = match author_logs.get_mut(&log_id) {
            Some(log) => log,
            // If there isn't one, then create and insert it.
            // Here we assume single writer scenario for now.
            // With multi-writers we will need to find the document
            // id by traversing the operation graph back to it's root.
            None => {
                author_logs.insert(log_id, Log::new(entry_encoded, operation_encoded));
                author_logs.get_mut(&log_id).unwrap()
            }
        };

        // Add this entry to database
        log.add_entry(LogEntry::new(
            entry_encoded.to_owned(),
            operation_encoded.to_owned(),
        ));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::{LogId, SeqNum};
    use crate::operation::OperationValue;
    use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;
    use crate::test_utils::fixtures::{create_operation, hash, private_key, update_operation};
    use crate::test_utils::mocks::client::Client;
    use crate::test_utils::utils::{keypair_from_private, operation_fields, NextEntryArgs};

    use super::{send_to_node, Node};

    #[rstest]
    fn next_entry_args(private_key: String) {
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
        let mut node = Node::new();

        let mut next_entry_args = node.next_entry_args(&panda.author(), None, None).unwrap();

        let mut expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(1).unwrap(),
            backlink: None,
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);

        // Publish a CREATE operation
        let entry1_hash = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                hash(DEFAULT_SCHEMA_HASH),
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Ohh, my first message!".to_string()),
                )]),
            ),
            None,
        )
        .unwrap();

        next_entry_args = node
            .next_entry_args(&panda.author(), Some(&entry1_hash), None)
            .expect("No entry args returned!");

        expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(2).unwrap(),
            backlink: Some(entry1_hash.clone()),
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);

        // Publish an UPDATE operation
        let entry2_hash = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                vec![entry1_hash.clone()],
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update.".to_string()),
                )]),
            ),
            Some(&entry1_hash.clone()),
        )
        .unwrap();

        next_entry_args = node
            .next_entry_args(&panda.author(), Some(&entry1_hash), None)
            .unwrap();

        expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(3).unwrap(),
            backlink: Some(entry2_hash),
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);
    }

    #[rstest]
    fn next_entry_args_at_specific_seq_num(private_key: String) {
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
        let mut node = Node::new();

        // Publish a CREATE operation
        let entry1_hash = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                hash(DEFAULT_SCHEMA_HASH),
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Ohh, my first message!".to_string()),
                )]),
            ),
            None,
        )
        .unwrap();

        // Publish an UPDATE operation
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                vec![entry1_hash.clone()],
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update.".to_string()),
                )]),
            ),
            Some(&entry1_hash.clone()),
        )
        .unwrap();

        let next_entry_args = node
            .next_entry_args(
                &panda.author(),
                Some(&entry1_hash),
                Some(&SeqNum::new(2).unwrap()),
            )
            .unwrap();

        let expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(2).unwrap(),
            backlink: Some(entry1_hash),
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);
    }
}
