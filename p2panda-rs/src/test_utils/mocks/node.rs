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
//! use p2panda_rs::schema::SchemaId;
//! use p2panda_rs::test_utils::constants::TEST_SCHEMA_ID;
//! use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
//! use p2panda_rs::test_utils::fixtures::{
//!     create_operation, delete_operation, schema, random_key_pair, operation_fields, update_operation,
//! };
//!
//! // Instantiate a new mock node
//! let mut node = Node::new();
//!
//! // Instantiate one client named "panda"
//! let panda = Client::new("panda".to_string(), random_key_pair());
//!
//! // Panda creates a new chat document by publishing a CREATE operation
//! let (document1_hash_id, _) = send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_operation(
//!         &[(
//!             "message",
//!             OperationValue::Text("Ohh, my first message!".to_string()),
//!         )],
//!     )
//! )
//! .unwrap();
//!
//! // Panda updates the document by publishing an UPDATE operation
//! let (entry2_hash, _) = send_to_node(
//!     &mut node,
//!     &panda,
//!     &update_operation(
//!         &[(
//!             "message",
//!             OperationValue::Text("Which I now update.".to_string()),
//!         )],
//!         &document1_hash_id.clone().into(),
//!     )
//! )
//! .unwrap();
//!
//! // Panda deletes their document by publishing a DELETE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &delete_operation(
//!         &entry2_hash.into()
//!     )
//! )
//! .unwrap();
//!
//! // Panda creates another chat document by publishing a new CREATE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_operation(
//!         &[(
//!             "message",
//!             OperationValue::Text("Let's try that again.".to_string()),
//!         )],
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
use async_std::task;
use log::{debug, info};

use std::collections::{HashMap, HashSet};

use crate::document::{Document, DocumentBuilder, DocumentId};
use crate::entry::{decode_entry, EntrySigned, LogId};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, Operation, OperationEncoded, OperationId, VerifiedOperation};
use crate::storage_provider::traits::test_utils::send_to_store;
use crate::storage_provider::traits::{
    AsStorageEntry, DocumentStore, EntryStore, OperationStore, StorageProvider,
};
use crate::test_utils::db::{
    EntryArgsRequest, EntryArgsResponse, PublishEntryRequest, PublishEntryResponse,
};
use crate::test_utils::db::{SimplestStorageProvider, StorageEntry};
use crate::test_utils::mocks::utils::Result;
use crate::test_utils::mocks::Client;

pub async fn process_new_operation(node: &mut Node, operation: &OperationId) -> Result<()> {
    let document_id = node
        .0
        .get_document_by_entry(operation.as_hash())
        .await?
        .expect("No document found for operation");
    // Now we perform materialisation on the effected document.
    let document_operations = node.0.get_operations_by_document_id(&document_id).await?;

    let document = DocumentBuilder::new(document_operations).build()?;
    node.0.insert_document(&document).await?;
    Ok(())
}

/// Helper method signing and encoding entry and sending it to node backend.
pub fn send_to_node(
    node: &mut Node,
    client: &Client,
    operation: &Operation,
) -> Result<(Hash, PublishEntryResponse)> {
    // Insert the entry, operation and log into the database.
    let (entry_encoded, response) =
        task::block_on(async { send_to_store(&node.0, operation, &client.key_pair).await })?;

    // Trigger materialisation by processing the new operation.
    task::block_on(async { process_new_operation(node, &entry_encoded.hash().into()).await })?;

    Ok((entry_encoded.hash(), response))
}

/// This node mocks functionality which would be implemented in a real world p2panda node.
///
/// It does so in a simplistic manner and should only be used in a testing environment or demo
/// environment.
#[derive(Debug, Default)]
pub struct Node(SimplestStorageProvider);

impl Node {
    /// Create a new mock Node.
    pub fn new() -> Self {
        Self(SimplestStorageProvider::default())
    }

    /// Return the entire store.
    pub fn db(&self) -> SimplestStorageProvider {
        self.0.clone()
    }

    /// Get entry by id
    pub fn get_entry(&self, id: &Hash) -> Option<StorageEntry> {
        task::block_on(async { self.0.get_entry_by_hash(id).await.unwrap() })
    }

    /// Get an array of all entries in database.
    pub fn all_entries(&self) -> Vec<StorageEntry> {
        self.0
            .entries
            .lock()
            .unwrap()
            .iter()
            .map(|(_, entry)| entry.clone())
            .collect()
    }

    pub fn get_authors(&self) -> HashSet<Author> {
        let mut authors = HashSet::new();
        let entries = self.0.entries.lock().unwrap();
        for (_, entry) in entries.iter() {
            authors.insert(entry.author());
        }
        authors
    }

    pub fn get_author_logs(&self, author: &Author) -> HashMap<LogId, HashSet<StorageEntry>> {
        let mut logs: HashMap<LogId, HashSet<StorageEntry>> = HashMap::new();
        let entries = self.0.entries.lock().unwrap();
        for (_, entry) in entries.iter() {
            if &entry.author() != author {
                continue;
            }
            let mut log_entries = logs.get_mut(&entry.log_id()).cloned().unwrap_or_default();
            log_entries.insert(entry.clone());
            logs.insert(entry.log_id(), log_entries);
        }
        logs
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
        document_id: Option<&DocumentId>,
    ) -> Result<EntryArgsResponse> {
        info!(
            "[next_entry_args] REQUEST: next entry args for author {} document {}",
            author.as_str(),
            document_id.map(|id| id.as_str()).unwrap_or("not provided"),
        );

        debug!("\n{:?}\n{:?}", author, document_id);

        let entry_args_request = EntryArgsRequest {
            public_key: author.clone(),
            document_id: document_id.cloned(),
        };

        let next_entry_args =
            task::block_on(async move { self.0.get_entry_args(&entry_args_request).await })?;

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

    /// Store an entry in the database and return the hash of the newly created entry.
    pub fn publish_entry(
        &mut self,
        entry_encoded: &EntrySigned,
        operation_encoded: &OperationEncoded,
    ) -> Result<PublishEntryResponse> {
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

        let publish_entry_request = PublishEntryRequest {
            entry: entry_encoded.clone(),
            operation: operation_encoded.clone(),
        };

        let publish_entry_response =
            task::block_on(async move { self.0.publish_entry(&publish_entry_request).await })?;

        info!(
            "[publish_entry] RESPONSE: succesfully published entry: {} to log: {} and returning next entry args",
            entry_encoded.hash().as_str(),
            log_id.as_u64()
        );

        debug!("\n{:?}", publish_entry_response);

        Ok(publish_entry_response)
    }

    /// Get a single resolved document from the node.
    pub fn get_document(&self, id: &DocumentId) -> Option<Document> {
        let operations: Vec<VerifiedOperation> = self
            .0
            .operations
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, (document_id, _))| document_id == id)
            .map(|(_, (_, operation))| operation.clone())
            .collect();

        if operations.is_empty() {
            return None;
        };
        Some(
            DocumentBuilder::new(operations)
                .build()
                .expect("Could not build document"),
        )
    }

    /// Get all documents in their resolved state from the node.
    pub fn get_documents(&self) -> Vec<Document> {
        let mut documents: HashSet<DocumentId> = HashSet::new();

        self.0
            .operations
            .lock()
            .unwrap()
            .iter()
            .for_each(|(_, (document_id, _))| {
                documents.insert(document_id.clone());
            });

        documents
            .iter()
            .flat_map(|id| self.get_document(&id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::entry::{LogId, SeqNum};
    use crate::identity::KeyPair;
    use crate::operation::OperationValue;
    use crate::test_utils::fixtures::{create_operation, key_pair, private_key, update_operation};
    use crate::test_utils::mocks::client::Client;
    use crate::test_utils::utils::NextEntryArgs;

    use super::{send_to_node, Node};

    #[rstest]
    fn publishing_entries(private_key: String) {
        let panda = Client::new("panda".to_string(), key_pair(&private_key));
        let mut node = Node::new();

        // This is an empty node which has no author logs.
        let next_entry_args = node.get_next_entry_args(&panda.author(), None).unwrap();

        // These are the next_entry_args we would expect to get when making a request to this node.
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

        // Panda publishes a create operation.
        // This instantiates a new document.
        //
        // PANDA  : [1]
        let (panda_entry_1_hash, next_entry_args) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[(
                "message",
                OperationValue::Text("Ohh, my first message! [Panda]".to_string()),
            )]),
        )
        .unwrap();

        // The document id is derived from the hash of it's first entry.
        let document_id = panda_entry_1_hash.clone().into();

        // The seq_num has incremented to 2 because panda already published one entry.
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

        // The database contains one author now.
        assert_eq!(node.get_authors().len(), 1);
        // Who has one log.
        assert_eq!(node.get_author_logs(&panda.author()).len(), 1);

        // Panda publishes an update operation.
        // It contains the hash of the current graph tip in it's `previous_operations`.
        //
        // PANDA  : [1] <-- [2]
        let (panda_entry_2_hash, next_entry_args) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[(
                    "message",
                    OperationValue::Text("Which I now update. [Panda]".to_string()),
                )],
                &panda_entry_1_hash.clone().into(),
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

        assert_eq!(node.get_authors().len(), 1);
        assert_eq!(node.get_author_logs(&panda.author()).len(), 1);

        let penguin = Client::new("penguin".to_string(), KeyPair::new());

        let next_entry_args = node
            .get_next_entry_args(&penguin.author(), Some(&document_id))
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

        // Penguin publishes an update operation which refers to panda's last operation
        // as the graph tip.
        //
        // PANDA  : [1] <--[2]
        // PENGUIN:           \--[1]
        let (penguin_entry_1_hash, next_entry_args) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                &[(
                    "message",
                    OperationValue::Text("My turn to update. [Penguin]".to_string()),
                )],
                &panda_entry_2_hash.into(),
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

        assert_eq!(node.get_authors().len(), 2);
        assert_eq!(node.get_author_logs(&penguin.author()).len(), 1);

        // Penguin publishes another update operation refering to their own previous operation
        // as the graph tip.
        //
        // PANDA  : [1] <--[2]
        // PENGUIN:           \--[1] <--[2]
        let (penguin_entry_2_hash, next_entry_args) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                &[(
                    "message",
                    OperationValue::Text("And again. [Penguin]".to_string()),
                )],
                &penguin_entry_1_hash.clone().into(),
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

        // Now there are 2 authors publishing ot the node.
        assert_eq!(node.get_authors().len(), 2);
        assert_eq!(node.get_author_logs(&penguin.author()).len(), 1);

        // We can query the node for the current document state.
        let document = node.get_document(&document_id).unwrap();
        let document_view_value = document.view().unwrap().get("message").unwrap();
        // It was last updated by Penguin, this writes over previous values.
        assert_eq!(
            document_view_value.value(),
            &OperationValue::Text("And again. [Penguin]".to_string())
        );
        // There should only be one document in the database.
        assert_eq!(node.get_documents().len(), 1);

        // Panda publishes another create operation.
        // This again instantiates a new document.
        //
        // PANDA  : [1]
        let (panda_entry_1_hash, next_entry_args) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[(
                "message",
                OperationValue::Text("Ohh, my first message in a new document!".to_string()),
            )]),
        )
        .unwrap();

        expected_next_entry_args = NextEntryArgs {
            log_id: LogId::new(2),
            seq_num: SeqNum::new(2).unwrap(),
            backlink: Some(panda_entry_1_hash),
            skiplink: None,
        };

        assert_eq!(next_entry_args.log_id, expected_next_entry_args.log_id);
        assert_eq!(next_entry_args.seq_num, expected_next_entry_args.seq_num);
        assert_eq!(next_entry_args.backlink, expected_next_entry_args.backlink);
        assert_eq!(next_entry_args.skiplink, expected_next_entry_args.skiplink);

        assert_eq!(node.get_authors().len(), 2);
        // Now panda has 2 document logs.
        assert_eq!(node.get_author_logs(&panda.author()).len(), 2);
        // There should be 2 document in the database.
        assert_eq!(node.get_documents().len(), 2);
    }

    #[rstest]
    fn concurrent_updates(private_key: String) {
        let panda = Client::new("panda".to_string(), key_pair(&private_key));
        let penguin = Client::new(
            "penguin".to_string(),
            key_pair("eb852fefa703901e42f17cdc2aa507947f392a72101b2c1a6d30023af14f75e3"),
        );
        let mut node = Node::new();

        // Publish a CREATE operation
        //
        // PANDA  : [1]
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(&[
                (
                    "cafe_name",
                    OperationValue::Text("Polar Pear Cafe".to_string()),
                ),
                (
                    "address",
                    OperationValue::Text("1, Polar Bear Rise, Panda Town".to_string()),
                ),
            ]),
        )
        .unwrap();

        let document = node
            .get_document(&panda_entry_1_hash.clone().into())
            .unwrap();
        let document_view_value = document.view().unwrap().get("cafe_name").unwrap();
        assert_eq!(
            document_view_value.value(),
            &OperationValue::Text("Polar Pear Cafe".to_string())
        );

        // Publish an UPDATE operation
        //
        // PANDA  : [1] <--[2]
        let (panda_entry_2_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                &[(
                    "cafe_name",
                    OperationValue::Text("Polar Bear Cafe".to_string()),
                )],
                &panda_entry_1_hash.clone().into(),
            ),
        )
        .unwrap();

        let document = node
            .get_document(&panda_entry_1_hash.clone().into())
            .unwrap();
        let document_view_value = document.view().unwrap().get("cafe_name").unwrap();
        assert_eq!(
            document_view_value.value(),
            &OperationValue::Text("Polar Bear Cafe".to_string())
        );

        // Penguin publishes an UPDATE operation, but they haven't seen Panda's most recent entry [2]
        // making this a concurrent update which forks the document graph.
        //
        // PANDA  : [1] <--[2]
        //            \
        // PENGUIN:    [1]
        let (penguin_entry_1_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                &[(
                    "address",
                    OperationValue::Text("1, Polar Bear rd, Panda Town".to_string()),
                )],
                &panda_entry_1_hash.clone().into(),
            ),
        )
        .unwrap();

        let document = node
            .get_document(&panda_entry_1_hash.clone().into())
            .unwrap();
        let document_view_value = document.view().unwrap().get("cafe_name").unwrap();
        assert_eq!(
            document_view_value.value(),
            &OperationValue::Text("Polar Bear Cafe".to_string())
        );

        // Penguin publishes another UPDATE operation, this time they have replicated all entries
        // and refer to the two existing document graph tips in the previous_operation fields.
        //
        // PANDA  : [1] <-- [2]
        //            \        \
        // PENGUIN:    [1] <-- [2]
        let (_penguin_entry_2_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                &[(
                    "cafe_name",
                    OperationValue::Text("Polar Bear Café".to_string()),
                )],
                &DocumentViewId::new(&[penguin_entry_1_hash.into(), panda_entry_2_hash.into()])
                    .unwrap(),
            ),
        )
        .unwrap();

        let document = node.get_document(&panda_entry_1_hash.into()).unwrap();
        let document_view_value = document.view().unwrap().get("cafe_name").unwrap();
        assert_eq!(
            document_view_value.value(),
            &OperationValue::Text("Polar Bear Café".to_string())
        );

        // As more operations are published, the graph could look like this:
        //
        // PANDA  : [1] <--[2]          [3] <--[4] <--[5]
        //            \       \         /
        // PENGUIN:    [1] <--[2] <--[3]
    }
}
