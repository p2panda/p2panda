// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda node.
//!
//! This node mocks functionality which would be implemented in a real world p2panda node.
//! It does so in a simplistic manner and should only be used in a testing environment or demo
//! environment.
//!
//! ## Example
//! ```
//! use p2panda_rs::test_utils::mocks::{Client, send_to_node, Node};
//! use p2panda_rs::test_utils::utils::{create_operation, delete_operation, hash, operation_fields,
//!     new_key_pair, update_operation
//! };
//! use p2panda_rs::test_utils::constants::DEFAULT_SCHEMA_HASH;
//! use p2panda_rs::operation::OperationValue;
//!
//! # const CHAT_SCHEMA_HASH: &str = DEFAULT_SCHEMA_HASH;
//!
//! // Instantiate a new mock node
//! let mut node = Node::new();
//!
//! // Instantiate one client named "panda"
//! let panda = Client::new("panda".to_string(), new_key_pair());
//!
//! // Panda creates a chat document by publishing a CREATE operation
//! let entry1_hash = send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         operation_fields(vec![("message", OperationValue::Text("Ohh, my first message!".to_string()))]),
//!     ),
//! )
//! .unwrap();
//!
//! // Panda updates their document by publishing an UPDATE operation
//! let entry2_hash = send_to_node(
//!     &mut node,
//!     &panda,
//!     &update_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         entry1_hash.clone(),
//!         vec![entry1_hash.clone()],
//!         operation_fields(vec![("message", OperationValue::Text("Which I now update.".to_string()))]),
//!     ),
//! )
//! .unwrap();
//!
//! // Panda deletes their document by publishing a DELETE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &delete_operation(hash(CHAT_SCHEMA_HASH), entry1_hash, vec![entry2_hash]),
//! )
//! .unwrap();
//!
//! // Panda creates another chat document by publishing a CREATE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_operation(
//!         hash(CHAT_SCHEMA_HASH),
//!         operation_fields(vec![("message", OperationValue::Text("Let's try that again.".to_string()))]),
//!     ),
//! )
//! .unwrap();
//!
//! // Get all entries published to this node
//! let entries = node.all_entries();
//!
//! // There should be 4 entries
//! entries.len(); // => 4
//!
//! // Query all instances of a certain schema
//! let instances = node.query_all(&CHAT_SCHEMA_HASH.to_string()).unwrap();
//!
//! // There should be one instance, because on was deleted
//! instances.len(); // => 1
//!
//! // Query for one instance by id
//! let instance = node
//!     .query(&CHAT_SCHEMA_HASH.to_string(), &entries[3].hash_str())
//!     .unwrap();
//!
//! instance.get("message").unwrap(); // => "Let's try that again."
//! ```
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
use std::collections::HashMap;

use crate::entry::{decode_entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, Operation, OperationFields};

use crate::test_utils::mocks::constants::{
    GROUP_SCHEMA_HASH, KEY_PACKAGE_SCHEMA_HASH, META_SCHEMA_HASH, PERMISSIONS_SCHEMA_HASH,
};
use crate::test_utils::mocks::logs::{Log, LogEntry};
use crate::test_utils::mocks::materialisation::{filter_entries, Materialiser};
use crate::test_utils::mocks::utils::Result;
use crate::test_utils::mocks::Client;
use crate::test_utils::utils::NextEntryArgs;

/// Helper method signing and encoding entry and sending it to node backend.
pub fn send_to_node(node: &mut Node, client: &Client, operation: &Operation) -> Result<Hash> {
    let entry_args = node.next_entry_args(&client.author(), &operation.schema(), None)?;

    let entry_encoded = client.signed_encoded_entry(operation.to_owned(), entry_args);

    node.publish_entry(&entry_encoded, operation)?;

    // Return entry hash for now so we can use it to perform UPDATE and DELETE operations later
    Ok(entry_encoded.hash())
}

/// Calculate the skiplink and backlink at a certain point in a log of entries
fn calculate_links(seq_num: &SeqNum, log: &Log) -> (Option<Hash>, Option<Hash>) {
    // Next skiplink hash
    let skiplink = match seq_num.skiplink_seq_num() {
        Some(seq) if is_lipmaa_required(seq_num.as_i64() as u64) => Some(
            log.entries()
                .get(seq.as_i64() as usize - 1)
                .expect("Skiplink missing!")
                .hash(),
        ),
        _ => None,
    };

    // Next backlink hash
    let backlink = seq_num.backlink_seq_num().map(|seq| {
        log.entries()
            .get(seq.as_i64() as usize - 1)
            .expect("Backlink missing!")
            .hash()
    });
    (backlink, skiplink)
}

/// Mock database type
pub type Database = HashMap<String, HashMap<i64, Log>>;

/// This node mocks functionality which would be implemented in a real world p2panda node.
/// It does so in a simplistic manner and should only be used in a testing environment or demo
/// environment.
#[derive(Debug, Default)]
pub struct Node {
    /// Internal database which maps authors and log ids to Bamboo logs with entries inside.
    entries: Database,
}

impl Node {
    /// Create a new mock Node
    pub fn new() -> Self {
        Self {
            entries: Database::new(),
        }
    }

    /// Get a mutable map of all logs published by a certain author
    fn get_author_logs_mut(&mut self, author: &Author) -> Option<&mut HashMap<i64, Log>> {
        let author_str = author.as_str();
        if !self.entries.contains_key(author_str) {
            return None;
        }
        Some(self.entries.get_mut(author_str).unwrap())
    }

    /// Get a map of all logs published by a certain author
    fn get_author_logs(&self, author: &Author) -> Option<&HashMap<i64, Log>> {
        let author_str = author.as_str();
        if !self.entries.contains_key(author_str) {
            return None;
        }
        Some(self.entries.get(author_str).unwrap())
    }

    /// Get the author of an Instance by instance id
    pub fn get_instance_author(&self, instance_id: String) -> Option<String> {
        let mut instance_author = None;
        self.entries.keys().for_each(|author| {
            let author_logs = self.entries.get(author).unwrap();
            author_logs.iter().for_each(|(_id, log)| {
                let entries = log.entries();
                let instance_create_entry = entries
                    .iter()
                    .find(|log_entry| log_entry.hash_str() == instance_id);
                if instance_create_entry.is_some() {
                    instance_author = Some(author.to_owned())
                };
            });
        });
        instance_author
    }

    /// Find the log id of the given schema, usually the mechanism would look a little different
    /// here, in our test demo we take the following steps
    pub fn get_log_id(&mut self, schema: &Hash, author: &Author) -> Result<LogId> {
        let author_logs = self.get_author_logs(author);

        let log_id = match schema.as_str() {
            // Check if the schema hash matches any of our hard coded system schema
            // if it does, return the hard coded id
            META_SCHEMA_HASH => 2,
            GROUP_SCHEMA_HASH => 4,
            KEY_PACKAGE_SCHEMA_HASH => 6,
            PERMISSIONS_SCHEMA_HASH => 8,
            // If it doesn't match it must be a user schema
            _ => match author_logs {
                Some(logs) => match logs.values().find(|log| log.schema() == schema.as_str()) {
                    // If a log with this hash already exists, return the existing id
                    Some(log) => log.id(),
                    None => {
                        // Otherwise find the highest existing user schema log id
                        let max_id = logs
                            .values()
                            // Filter out all even (system log) values
                            .filter(|log| log.id() % 2 != 0)
                            .map(|log| log.id())
                            .max();
                        match max_id {
                            // And add 2
                            Some(mut id) => {
                                id += 2;
                                id
                            }
                            // If there aren't any then this is the first log, return 1
                            None => 1,
                        }
                    }
                },
                // If there aren't any then this is the first log, return 1
                None => 1,
            },
        };
        Ok(LogId::new(log_id))
    }

    /// Get an array of all entries in database
    pub fn all_entries(&self) -> Vec<LogEntry> {
        let mut all_entries: Vec<LogEntry> = Vec::new();
        self.entries.iter().for_each(|(_id, author_logs)| {
            author_logs
                .iter()
                .for_each(|(_id, log)| all_entries.append(log.entries().as_mut()))
        });
        all_entries
    }

    /// Return the entire database
    pub fn db(&self) -> Database {
        self.entries.clone()
    }

    /// Returns the log id, sequence number, skiplink and backlink hash for a given author and
    /// schema. All of this information is needed to create and sign a new entry.
    ///
    /// If a value for the optional seq_num parameter is passed then next entry args *at that point* in this log
    /// are returned. This is helpful when generating test data and wanting to test the flow from requesting entry
    /// args through to publishing an entry.
    pub fn next_entry_args(
        &mut self,
        author: &Author,
        schema: &Hash,
        seq_num: Option<&SeqNum>,
    ) -> Result<NextEntryArgs> {
        // Find out the log id for the given schema
        let log_id = self.get_log_id(schema, author)?;

        // Find any logs by this author for this schema
        let author_log = match self.get_author_logs_mut(author) {
            Some(logs) => logs.values().find(|log| log.schema() == schema.as_str()),
            // No logs for this author
            None => None,
        };

        // Find out the sequence number, skip- and backlink hash for the next entry in this log
        let entry_args = match author_log {
            Some(log) => {
                let seq_num_inner = match seq_num {
                    // If a sequence number was passed...
                    Some(s) => {
                        // ...trim the log to the point in time we are interested in
                        log.to_owned().entries =
                            log.entries()[..s.as_i64() as usize - 1].to_owned();
                        // and return the sequence number.
                        s.to_owned()
                    }
                    None => {
                        // If no sequence number was passed calculate and return the next sequence number for this log
                        SeqNum::new((log.entries().len() + 1) as i64).unwrap()
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

    /// Get the next instance args (hash of the entry considered the tip of this instance) needed when publishing
    /// UPDATE or DELETE operations
    pub fn next_instance_args(&mut self, instance_id: &str) -> Option<String> {
        let mut materialiser = Materialiser::new();
        let filtered_entries = filter_entries(self.all_entries());
        materialiser.build_dags(filtered_entries);
        // Get the instance with this id
        match materialiser.dags().get_mut(instance_id) {
            // Sort it topologically and take the last entry hash
            Some(instance_dag) => instance_dag.topological().pop(),
            None => None,
        }
    }

    /// Store entry in the database. Please note that since this an experimental implemention this
    /// method does not validate any integrity or content of the given entry.
    pub fn publish_entry(
        &mut self,
        entry_encoded: &EntrySigned,
        operation: &Operation,
    ) -> Result<()> {
        // We add on several metadata values that don't currently exist in a p2panda Entry.
        // Notably: previous_operation and instance_author

        let previous_operation = match operation.id() {
            Some(id) => self.next_instance_args(id.as_str()),
            None => None,
        };

        let entry = decode_entry(entry_encoded, None)?;
        let log_id = entry.log_id().as_i64();
        let author = entry_encoded.author();

        let instance_author = match operation.id() {
            Some(id) => self.get_instance_author(id.as_str().into()),
            None => Some(author.as_str().to_string()),
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

        // Get the log for this schema from the author logs
        let log = match author_logs.get_mut(&log_id) {
            Some(log) => log,
            // If there isn't one, then create and insert it
            None => {
                author_logs.insert(log_id, Log::new(log_id, operation.schema().as_str().into()));
                author_logs.get_mut(&log_id).unwrap()
            }
        };

        // Add this entry to database
        log.add_entry(LogEntry::new(
            author,
            instance_author,
            entry_encoded.to_owned(),
            operation.to_owned(),
            previous_operation,
        ));

        Ok(())
    }

    /// Get all Instances of this Schema
    pub fn query_all(&self, schema: &str) -> Result<HashMap<String, OperationFields>> {
        // Instantiate a new materialiser instance
        let mut materialiser = Materialiser::new();

        // Filter published entries against permissions published to user system log
        let filtered_entries = filter_entries(self.all_entries());

        // Materialise Instances resolving merging concurrent edits
        materialiser.materialise(&filtered_entries)?;

        // Query the materialised Instances
        materialiser.query_all(schema)
    }

    /// Get a specific Instance
    pub fn query(&self, schema: &str, instance: &str) -> Result<OperationFields> {
        // Instantiate a new materialiser instance
        let mut materialiser = Materialiser::new();

        // Filter published entries against permissions published to user system log
        let filtered_entries = filter_entries(self.all_entries());

        // Materialise Instances resolving merging concurrent edits
        materialiser.materialise(&filtered_entries)?;

        // Query the materialised Instances
        materialiser.query_instance(schema, instance)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::{LogId, SeqNum};
    use crate::operation::OperationValue;
    use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, hash, private_key, some_hash, update_operation,
    };
    use crate::test_utils::mocks::client::Client;
    use crate::test_utils::mocks::constants::{
        GROUP_SCHEMA_HASH, KEY_PACKAGE_SCHEMA_HASH, META_SCHEMA_HASH, PERMISSIONS_SCHEMA_HASH,
    };
    use crate::test_utils::mocks::node::{send_to_node, Node};
    use crate::test_utils::utils::{keypair_from_private, operation_fields, NextEntryArgs};

    fn mock_node(panda: &Client) -> Node {
        let mut node = Node::new();

        // Publish a CREATE operation
        let operation_1 = send_to_node(
            &mut node,
            panda,
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
        let operation_2 = send_to_node(
            &mut node,
            panda,
            &update_operation(
                hash(DEFAULT_SCHEMA_HASH),
                operation_1.clone(),
                vec![operation_1.clone()],
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Which I now update.".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Publish an DELETE operation
        send_to_node(
            &mut node,
            panda,
            &delete_operation(hash(DEFAULT_SCHEMA_HASH), operation_1, vec![operation_2]),
        )
        .unwrap();

        // Publish another CREATE operation
        send_to_node(
            &mut node,
            panda,
            &create_operation(
                hash(DEFAULT_SCHEMA_HASH),
                operation_fields(vec![(
                    "message",
                    OperationValue::Text("Let's try that again.".to_string()),
                )]),
            ),
        )
        .unwrap();

        node
    }

    #[rstest]
    fn get_log_id(private_key: String) {
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
        let mut node = mock_node(&panda);

        let log_id = node
            .get_log_id(&hash(DEFAULT_SCHEMA_HASH), &panda.author())
            .unwrap();
        let meta_schema_log_id = node
            .get_log_id(&hash(META_SCHEMA_HASH), &panda.author())
            .unwrap();
        let group_schema_log_id = node
            .get_log_id(&hash(GROUP_SCHEMA_HASH), &panda.author())
            .unwrap();
        let key_package_schema_log_id = node
            .get_log_id(&hash(KEY_PACKAGE_SCHEMA_HASH), &panda.author())
            .unwrap();
        let permissions_schema_log_id = node
            .get_log_id(&hash(PERMISSIONS_SCHEMA_HASH), &panda.author())
            .unwrap();

        assert_eq!(log_id, LogId::new(1));
        assert_eq!(meta_schema_log_id, LogId::new(2));
        assert_eq!(group_schema_log_id, LogId::new(4));
        assert_eq!(key_package_schema_log_id, LogId::new(6));
        assert_eq!(permissions_schema_log_id, LogId::new(8));
    }

    #[rstest]
    fn next_entry_args(private_key: String) {
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
        let mut node = mock_node(&panda);

        let next_entry_args = node
            .next_entry_args(&panda.author(), &hash(DEFAULT_SCHEMA_HASH), None)
            .unwrap();

        let expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(5).unwrap(),
            backlink: some_hash(
                "0020561988f9eaa7f0963a5cf86d5736f3c7b7b52b5346793c6fd71b5fce7223408e",
            ),
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
        let mut node = mock_node(&panda);

        let next_entry_args = node
            .next_entry_args(
                &panda.author(),
                &hash(DEFAULT_SCHEMA_HASH),
                Some(&SeqNum::new(3).unwrap()),
            )
            .unwrap();

        let expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(1),
            seq_num: SeqNum::new(3).unwrap(),
            backlink: some_hash(
                "0020460f9ec29237ce004093927ae066daa2f24b96ba4400a044a3c46092959c5725",
            ),
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);
    }

    #[rstest]
    fn query(private_key: String) {
        let panda = Client::new("panda".to_string(), keypair_from_private(private_key));
        let node = mock_node(&panda);

        // Get all entries
        let entries = node.all_entries();

        // There should be 4 entries
        assert_eq!(entries.len(), 4);

        // Query all instances
        let instances = node.query_all(&DEFAULT_SCHEMA_HASH.to_string()).unwrap();

        // There should be one instance
        assert_eq!(instances.len(), 1);

        // Query for one instance by id
        let instance = node
            .query(&DEFAULT_SCHEMA_HASH.to_string(), &entries[3].hash_str())
            .unwrap();

        // Operation content should be correct
        assert_eq!(
            instance.get("message").unwrap().to_owned(),
            OperationValue::Text("Let's try that again.".to_string())
        );
    }
}
