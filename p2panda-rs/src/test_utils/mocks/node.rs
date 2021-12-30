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
//! )
//! .unwrap();
//!
//! // Panda updates the document by publishing an UPDATE operation
//! let entry2_hash = send_to_node(
//!     &mut node,
//!     &panda,
//!     &update_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         document1_hash_id.clone(),
//!         vec![document1_hash_id.clone()],
//!         operation_fields(vec![(
//!             "message",
//!             OperationValue::Text("Which I now update.".to_string()),
//!         )]),
//!     ),
//! )
//! .unwrap();
//!
//! // Panda deletes their document by publishing a DELETE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &delete_operation(hash(CHAT_SCHEMA_HASH), document1_hash_id, vec![entry2_hash]),
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

use crate::entry::{decode_entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::materialiser::{marshall_entries, DAG};
use crate::operation::{AsOperation, Operation};
use crate::test_utils::mocks::logs::{Log, LogEntry};
use crate::test_utils::mocks::utils::Result;
use crate::test_utils::mocks::Client;
use crate::test_utils::utils::NextEntryArgs;

/// Helper method signing and encoding entry and sending it to node backend.
pub fn send_to_node(node: &mut Node, client: &Client, operation: &Operation) -> Result<Hash> {
    let entry_args = node.next_entry_args(&client.author(), operation.id(), None)?;

    let entry_encoded = client.signed_encoded_entry(operation.to_owned(), entry_args);

    node.publish_entry(&entry_encoded, operation)?;

    // Return entry hash for now so we can use it to perform UPDATE and DELETE operations later
    Ok(entry_encoded.hash())
}

/// Calculate the skiplink and backlink at a certain point in a log of entries.
fn calculate_links(seq_num: &SeqNum, log: &Log) -> (Option<Hash>, Option<Hash>) {
    // Next skiplink hash
    let skiplink = match seq_num.skiplink_seq_num() {
        Some(seq) if is_lipmaa_required(seq_num.as_u64()) => Some(
            log.entries()
                .get(seq.as_u64() as usize - 1)
                .expect("Skiplink missing!")
                .hash(),
        ),
        _ => None,
    };

    // Next backlink hash
    let backlink = seq_num.backlink_seq_num().map(|seq| {
        log.entries()
            .get(seq.as_u64() as usize - 1)
            .expect("Backlink missing!")
            .hash()
    });
    (backlink, skiplink)
}

/// Mock database type.
pub type Database = HashMap<String, HashMap<u64, Log>>;

/// This node mocks functionality which would be implemented in a real world p2panda node.
///
/// It does so in a simplistic manner and should only be used in a testing environment or demo
/// environment.
#[derive(Debug, Default)]
pub struct Node {
    /// Internal database which maps authors and log ids to Bamboo logs with entries inside.
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
    fn get_author_logs_mut(&mut self, author: &Author) -> Option<&mut HashMap<u64, Log>> {
        let author_str = author.as_str();
        if !self.entries.contains_key(author_str) {
            return None;
        }
        Some(self.entries.get_mut(author_str).unwrap())
    }

    /// Get a map of all logs published by a certain author.
    fn get_author_logs(&self, author: &Author) -> Option<&HashMap<u64, Log>> {
        let author_str = author.as_str();
        if !self.entries.contains_key(author_str) {
            return None;
        }
        Some(self.entries.get(author_str).unwrap())
    }

    /// Get the author of a document.
    pub fn get_document_author(&self, document_id: String) -> Option<String> {
        let mut document_author = None;
        self.entries.keys().for_each(|author| {
            let author_logs = self.entries.get(author).unwrap();
            author_logs.iter().for_each(|(_id, log)| {
                let entries = log.entries();
                let document_create_entry = entries
                    .iter()
                    .find(|log_entry| log_entry.hash_str() == document_id);
                if document_create_entry.is_some() {
                    document_author = Some(author.to_owned())
                };
            });
        });
        document_author
    }

    /// Find the log id for the given document and author.
    pub fn get_log_id(&mut self, document_id: Option<&Hash>, author: &Author) -> Result<LogId> {
        match self.get_author_logs(author) {
            Some(logs) => {
                // Find the highest existing log id and increment it
                let next_free_log_id = logs.values().map(|log| log.id()).max().unwrap() + 1;

                match document_id {
                    Some(document) => {
                        let log_id = match logs
                            .values()
                            .find(|log| log.document() == document.as_str())
                        {
                            // If a log with this hash already exists, return the existing id
                            Some(log) => log.id(),
                            // Otherwise return the next free one
                            None => next_free_log_id,
                        };

                        Ok(LogId::new(log_id))
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
                    Some(logs) => logs
                        .values()
                        .find(|log| log.document() == document.as_str()),
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
                let seq_num_inner = match seq_num {
                    // If a sequence number was passed ...
                    Some(s) => {
                        // ... trim the log to the point in time we are interested in
                        log.to_owned().entries =
                            log.entries()[..s.as_u64() as usize - 1].to_owned();
                        // ... and return the sequence number.
                        s.to_owned()
                    }
                    None => {
                        // If no sequence number was passed calculate and return the next sequence
                        // number for this log
                        SeqNum::new((log.entries().len() + 1) as u64).unwrap()
                    }
                };

                // Calculate backlink and skiplink
                let (backlink, skiplink) = calculate_links(&seq_num_inner, log);

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

    /// Get the hash of the current tip of a document. Needed when publishing UPDATE or DELETE
    /// operations.
    pub fn get_document_tip(&mut self, document_id: &str) -> Option<String> {
        let edges = marshall_entries(
            self.all_entries()
                .iter()
                .filter(|entry| entry.hash().as_str() == document_id)
                .map(|entry| (entry.entry_encoded(), entry.operation_encoded()))
                .collect(),
        )
        .unwrap();
        if !edges.is_empty() {
            let mut dag = DAG::new_from_graph(edges);
            dag.topological().pop()
        } else {
            None
        }
    }

    /// Store entry in the database.
    ///
    /// Please note that since this an experimental implementation this method does not validate any
    /// integrity or content of the given entry.
    pub fn publish_entry(
        &mut self,
        entry_encoded: &EntrySigned,
        operation: &Operation,
    ) -> Result<()> {
        // We add on several metadata values that don't currently exist in a p2panda Entry.
        // Notably: previous_operation and document_author

        let previous_operation = match operation.id() {
            Some(id) => self.get_document_tip(id.as_str()),
            None => None,
        };

        let entry = decode_entry(entry_encoded, None)?;
        let log_id = entry.log_id().as_u64();
        let author = entry_encoded.author();

        let document_author = match operation.id() {
            Some(id) => self.get_document_author(id.as_str().into()),
            None => Some(author.as_str().to_string()),
        };

        let document_id = if operation.is_create() {
            entry_encoded.hash()
        } else {
            operation.id().unwrap().to_owned()
        };

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
            // If there isn't one, then create and insert it
            None => {
                author_logs.insert(
                    log_id,
                    Log::new(
                        log_id,
                        operation.schema().as_str().into(),
                        document_id.as_str().into(),
                    ),
                );
                author_logs.get_mut(&log_id).unwrap()
            }
        };

        // Add this entry to database
        log.add_entry(LogEntry::new(
            author,
            document_author,
            entry_encoded.to_owned(),
            operation.to_owned(),
            previous_operation,
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
        )
        .unwrap();

        next_entry_args = node
            .next_entry_args(&panda.author(), Some(&entry1_hash), None)
            .unwrap();

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
                entry1_hash.clone(),
                vec![entry1_hash.clone()],
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update.".to_string()),
                )]),
            ),
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
        )
        .unwrap();

        // Publish an UPDATE operation
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                entry1_hash.clone(),
                vec![entry1_hash.clone()],
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update.".to_string()),
                )]),
            ),
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
