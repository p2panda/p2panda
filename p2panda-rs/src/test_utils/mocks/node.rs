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
//! let (document1_hash_id, _) = send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         operation_fields(vec![(
//!             "message",
//!             OperationValue::Text("Ohh, my first message!".to_string()),
//!         )]),
//!     )
//! )
//! .unwrap();
//!
//! // Panda updates the document by publishing an UPDATE operation
//! let (entry2_hash, _) = send_to_node(
//!     &mut node,
//!     &panda,
//!     &update_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         vec![document1_hash_id.clone()],
//!         operation_fields(vec![(
//!             "message",
//!             OperationValue::Text("Which I now update.".to_string()),
//!         )]),
//!     )
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
//!     )
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
//!     )
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
use log::{debug, info};

use std::collections::HashMap;
use std::convert::TryFrom;

use crate::entry::{decode_entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, Operation, OperationEncoded};
use crate::test_utils::mocks::logs::{AuthorLogs, LogEntry};
use crate::test_utils::mocks::utils::Result;
use crate::test_utils::mocks::Client;
use crate::test_utils::utils::NextEntryArgs;

/// Helper method signing and encoding entry and sending it to node backend.
pub fn send_to_node(
    node: &mut Node,
    client: &Client,
    operation: &Operation,
) -> Result<(Hash, NextEntryArgs)> {
    // We need to establish which document this operation is targeting before proceeding.
    // First we check if this is a create message, which would mean no document exists yet.
    let document_id = if operation.is_create() {
        None
    } else {
        // If this isn't a create message, then there must be an existing document
        // this operation to be valid.

        // We get the previous_operations field first.
        let previous_operations = operation
            .previous_operations()
            .expect("UPDATE / DELETE operations must contain previous_operations");

        // If it's an empty collection we have a problem as all UPDATE and DELETE operations
        // must be pointing at other existing operations.
        if previous_operations.is_empty() {
            return Err(
                "UPDATE / DELETE operations must have more than 1 previous operation".into(),
            );
        };

        // Using the first previous operation in the list we retrieve the associated document
        // id from the database.
        let document_id = node.get_document_by_entry(&previous_operations[0]);

        Some(document_id.expect("This node does not contain the required document"))
    };

    // Here we can retrieve the correct entry arguments for constructing an entry.
    let entry_args = node.get_next_entry_args(&client.author(), document_id.as_ref(), None)?;

    // The entry is constructed, signed and encoded.
    let entry_encoded = client.signed_encoded_entry(operation.to_owned(), entry_args);

    // The operation is also encoded.
    let operation_encoded = OperationEncoded::try_from(operation).unwrap();

    // Both are published to the node.
    let next_entry_args = node.publish_entry(&entry_encoded, &operation_encoded)?;

    // Return entry hash so we can use it to perform UPDATE and DELETE operations later.
    // @TODO: We really want to return the next entry args here which would include
    // the document graph tips. This requires integrating Document into the test utils.
    Ok((entry_encoded.hash(), next_entry_args))
}

/// Calculate the skiplink and backlink at a certain point in a log of entries.
fn calculate_links(seq_num: &SeqNum, log: &[LogEntry]) -> (Option<Hash>, Option<Hash>) {
    // Next skiplink hash
    let skiplink = match seq_num.skiplink_seq_num() {
        Some(seq) if is_lipmaa_required(seq_num.as_u64()) => Some(
            log.get(seq.as_u64() as usize - 1)
                .expect("Skiplink missing!")
                .hash(),
        ),
        _ => None,
    };

    // Next backlink hash
    let backlink = seq_num.backlink_seq_num().map(|seq| {
        log.get(seq.as_u64() as usize - 1)
            .expect("Backlink missing!")
            .hash()
    });
    (backlink, skiplink)
}

/// Mock database type.
///
/// Maps the public key of an author against a map of their logs identified by log id.
pub type Database = HashMap<String, AuthorLogs>;

/// This node mocks functionality which would be implemented in a real world p2panda node.
///
/// It does so in a simplistic manner and should only be used in a testing environment or demo
/// environment.
#[derive(Debug, Default)]
pub struct Node {
    /// Internal database structure.
    db: Database,
}

impl Node {
    /// Create a new mock Node.
    pub fn new() -> Self {
        Self {
            db: Database::new(),
        }
    }

    /// Return the entire database.
    pub fn db(&self) -> Database {
        self.db.clone()
    }

    /// Get a mutable map of all logs published by a certain author.
    fn get_author_logs_mut(&mut self, author: &Author) -> Option<&mut AuthorLogs> {
        self.db.get_mut(author.as_str())
    }

    /// Get a map of all logs published by a certain author.
    fn get_author_logs(&self, author: &Author) -> Option<&AuthorLogs> {
        self.db.get(author.as_str())
    }

    /// Get the document id associated with the passed entry hash.
    fn get_document_by_entry(&self, entry: &Hash) -> Option<Hash> {
        let mut document_id = None;
        self.db.iter().any(|(_author, logs)| {
            let document_log = logs.find_document_log_by_entry(entry);
            match document_log {
                Some(log) => {
                    document_id = Some(log.document());
                    true
                }
                None => false,
            }
        });
        document_id
    }

    /// Get an array of all entries in database.
    pub fn all_entries(&self) -> Vec<LogEntry> {
        let mut all_entries: Vec<LogEntry> = Vec::new();
        self.db.iter().for_each(|(_id, author_logs)| {
            author_logs
                .iter()
                .for_each(|log| all_entries.append(log.entries().as_mut()))
        });
        all_entries
    }

    /// Public wrapper with logging for private next_entry_args method.
    ///
    /// Returns the log id, sequence number, skiplink and backlink hash for a given author and
    /// document. All of this information is needed to create and sign a new entry.
    ///
    /// If a value for the optional seq_num parameter is passed then next entry args *at that
    /// point* in this log are returned. This is helpful when generating test data and wanting to
    /// test the flow from requesting entry args through to publishing an entry.
    pub fn get_next_entry_args(
        &self,
        author: &Author,
        document_id: Option<&Hash>,
        seq_num: Option<&SeqNum>,
    ) -> Result<NextEntryArgs> {
        info!(
            "[next_entry_args] REQUEST: next entry args for author {} document {} {}",
            author.as_str(),
            document_id.map(|id| id.as_str()).unwrap_or("not provided"),
            seq_num
                .map(|seq_num| format!("at sequence number {}", seq_num.as_u64()))
                .unwrap_or_else(|| "".into())
        );

        debug!("\n{:?}\n{:?}\n{:?}", author, document_id, seq_num);

        let next_entry_args = self.next_entry_args(author, document_id, seq_num)?;

        info!(
            "[next_entry_args] RESPONSE: log id: {} seq num: {} backlink: {} skiplink: {}",
            next_entry_args.log_id.as_u64(),
            next_entry_args.seq_num.as_u64(),
            next_entry_args
                .backlink
                .as_ref()
                .map(|hash| hash.as_str())
                .unwrap_or("none"),
            next_entry_args
                .skiplink
                .as_ref()
                .map(|hash| hash.as_str())
                .unwrap_or("none"),
        );

        debug!("\n{:?}", next_entry_args);

        Ok(next_entry_args)
    }

    /// Returns the log id, sequence number, skiplink and backlink hash for a given author and
    /// document. All of this information is needed to create and sign a new entry.
    ///
    /// If a value for the optional seq_num parameter is passed then next entry args *at that
    /// point* in this log are returned. This is helpful when generating test data and wanting to
    /// test the flow from requesting entry args through to publishing an entry.
    fn next_entry_args(
        &self,
        author: &Author,
        document_id: Option<&Hash>,
        seq_num: Option<&SeqNum>,
    ) -> Result<NextEntryArgs> {
        // Get or instantiate a collection of logs for this author.
        let author_logs = match self.get_author_logs(author) {
            Some(logs) => logs.clone(),
            None => AuthorLogs::new(),
        };

        // Find the log for this document and author if it exists.
        let document_log = match document_id {
            Some(document_id) => author_logs.get_log_by_document_id(document_id),
            None => None,
        };

        // Construct the next entry args.
        let entry_args = match document_log {
            Some(log) => {
                // If a document log already we retrieve all the entries.
                let mut entries = log.entries();
                // If a seq num was passed to this method it means we are
                // requesting entry args for a specific point in this log.
                // NB: This is a functionality only implemented in the mock node
                //for testing purposes.
                let seq_num_inner = match seq_num {
                    // If a sequence number was passed ...
                    Some(s) => {
                        // ... trim the log to the point in time we are interested in
                        entries = entries[..s.as_u64() as usize - 1].to_owned();
                        // ... and return the sequence number.
                        *s
                    }
                    None => {
                        // If no sequence number was passed calculate and return the next sequence
                        // number for this log
                        log.next_seq_num()
                    }
                };

                // Calculate backlink and skiplink.
                let (backlink, skiplink) = calculate_links(&seq_num_inner, &entries);

                // Construct the next entry args.
                NextEntryArgs {
                    log_id: log.id(),
                    seq_num: seq_num_inner,
                    skiplink,
                    backlink,
                }
            }
            // This is the first entry in the log so we construct next entry args
            // from default values.
            None => NextEntryArgs {
                log_id: LogId::default(),
                seq_num: SeqNum::default(),
                skiplink: None,
                backlink: None,
            },
        };

        Ok(entry_args)
    }

    /// Store an entry in the database and return the hash of the newly created entry.
    pub fn publish_entry(
        &mut self,
        entry_encoded: &EntrySigned,
        operation_encoded: &OperationEncoded,
    ) -> Result<NextEntryArgs> {
        let entry = decode_entry(entry_encoded, Some(operation_encoded))?;
        let log_id = entry.log_id();
        let author = entry_encoded.author();
        let operation = entry.operation().unwrap();

        info!(
            "[publish_entry] REQUEST: publish entry: {} from author: {}",
            entry_encoded.hash().as_str(),
            author.as_str()
        );

        debug!("\n{:?}\n{:?}", entry_encoded, operation_encoded);

        let document_id = if !operation.is_create() {
            let previous_operations = operation.previous_operations().unwrap_or_else(|| {
                panic!(
                    "Document log for entry {} not found on node",
                    entry_encoded.hash().as_str()
                )
            });
            let document_id = self
                .get_document_by_entry(&previous_operations[0])
                .unwrap_or_else(|| {
                    panic!(
                        "Document log for entry {} not found on node",
                        entry_encoded.hash().as_str()
                    )
                });
            info!("Document found with id {}", document_id.as_str());
            document_id
        } else {
            info!(
                "Creating new document with id {}",
                entry_encoded.hash().as_str()
            );

            entry_encoded.hash()
        };

        // Get all logs by this author.
        let author_logs = match self.get_author_logs_mut(&author) {
            Some(logs) => logs,
            // If there aren't any, then instantiate a new log collection
            // and insert it into the db.
            None => {
                self.db.insert(author.as_str().into(), AuthorLogs::new());
                self.get_author_logs_mut(&author).unwrap()
            }
        };

        // Get the log for this document from the author logs.
        match author_logs.get_log_mut(log_id) {
            Some(log) => {
                // If there is one, insert this new entry.
                log.add_entry(LogEntry::new(entry_encoded, operation_encoded));
            }
            None => {
                // If there isn't one, then create and insert it.

                // First checking if the passed log id matches what we expect the next log
                // id for this log to be.
                let expected_log_id = author_logs.next_log_id();

                if *log_id != expected_log_id {
                    return Err(format!(
                        "Passed log id {} does not match expected log id {}",
                        log_id.as_u64(),
                        expected_log_id.as_u64()
                    )
                    .into());
                };

                // If it matches then we now create and insert the new log with it's
                // first entry included.
                author_logs.create_new_log(document_id.clone(), entry_encoded, operation_encoded);
            }
        };

        let next_entry_args = self.next_entry_args(&author, Some(&document_id), None)?;

        info!(
            "[publish_entry] RESPONSE: succesfully published entry: {} to log: {} and returning next entry args",
            entry_encoded.hash().as_str(),
            log_id.as_u64()
        );

        debug!("\n{:?}", next_entry_args);

        Ok(next_entry_args)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::{LogId, SeqNum};
    use crate::identity::KeyPair;
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

        let next_entry_args = node
            .get_next_entry_args(&panda.author(), None, None)
            .unwrap();

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
        let (panda_entry_1_hash, next_entry_args) = send_to_node(
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

        expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(2).unwrap(),
            backlink: Some(panda_entry_1_hash.clone()),
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);

        // Publish an UPDATE operation
        let (panda_entry_2_hash, next_entry_args) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                vec![panda_entry_1_hash.clone()],
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update.".to_string()),
                )]),
            ),
        )
        .unwrap();

        expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(3).unwrap(),
            backlink: Some(panda_entry_2_hash.clone()),
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);

        let penguin = Client::new("penguin".to_string(), KeyPair::new());

        let next_entry_args = node
            .next_entry_args(&penguin.author(), Some(&panda_entry_1_hash), None)
            .unwrap();

        expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(1).unwrap(),
            backlink: None,
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);

        // Publish an UPDATE operation
        let (penguin_entry_1_hash, next_entry_args) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                vec![panda_entry_2_hash],
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update.".to_string()),
                )]),
            ),
        )
        .unwrap();

        expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(2).unwrap(),
            backlink: Some(penguin_entry_1_hash.clone()),
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);

        // Publish an UPDATE operation
        let (penguin_entry_2_hash, next_entry_args) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                vec![penguin_entry_1_hash],
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update again.".to_string()),
                )]),
            ),
        )
        .unwrap();

        expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(3).unwrap(),
            backlink: Some(penguin_entry_2_hash),
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
        let (entry1_hash, _) = send_to_node(
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
