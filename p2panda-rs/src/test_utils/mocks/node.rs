// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda node.
//!
//! This node mocks functionality which would be implemented in a real world p2panda node. It does
//! so in a simplistic manner and should only be used in a testing environment or demo environment.
//!
/// TODO: Needs reinstating when we deal with https://github.com/p2panda/p2panda/issues/418
/// ## Example
///
/// # extern crate p2panda_rs;
/// # #[tokio::main]
/// # async fn main() -> p2panda_rs::storage_provider::utils::Result<()> {
/// use p2panda_rs::operation::OperationValue;
/// use p2panda_rs::schema::SchemaId;
/// use p2panda_rs::test_utils::constants::SCHEMA_ID;
/// use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
/// use p2panda_rs::test_utils::fixtures::{
///     create_operation, delete_operation, schema, random_key_pair, operation_fields, update_operation,
/// };
///
/// // Instantiate a new mock node
/// let mut node = Node::new();
///
/// // Instantiate one client named "panda"
/// let panda = Client::new("panda".to_string(), random_key_pair());
///
/// // Panda creates a new chat document by publishing a CREATE operation
/// let (document1_hash_id, _) = send_to_node(
///     &mut node,
///     &panda,
///     &create_operation(
///         &[(
///             "message",
///             OperationValue::Text("Ohh, my first message!".to_string()),
///         )],
///     )
/// ).await?;
///
/// // Panda updates the document by publishing an UPDATE operation
/// let (entry2_hash, _) = send_to_node(
///     &mut node,
///     &panda,
///     &update_operation(
///         &[(
///             "message",
///             OperationValue::Text("Which I now update.".to_string()),
///         )],
///         &document1_hash_id.clone().into(),
///     )
/// ).await?;
///
/// // Panda deletes their document by publishing a DELETE operation
/// send_to_node(
///     &mut node,
///     &panda,
///     &delete_operation(
///         &entry2_hash.into()
///     )
/// ).await?;
///
/// // Panda creates another chat document by publishing a new CREATE operation
/// send_to_node(
///     &mut node,
///     &panda,
///     &create_operation(
///         &[(
///             "message",
///             OperationValue::Text("Let's try that again.".to_string()),
///         )],
///     )
/// ).await?;
///
/// // Get all entries published to this node
/// let entries = node.entries();
///
/// // There should be 4 entries
/// entries.len(); // => 4
///
/// # Ok(())
/// # }
use std::collections::{HashMap, HashSet};

use crate::document::{Document, DocumentBuilder, DocumentId, DocumentView, DocumentViewId};
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::EncodedEntry;
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{EncodedOperation, Operation, OperationId, VerifiedOperation};
use crate::storage_provider::traits::test_utils::send_to_store;
use crate::storage_provider::traits::{DocumentStore, OperationStore, StorageProvider};
use crate::storage_provider::utils::Result;
use crate::test_utils::db::{EntryArgsResponse, PublishEntryResponse};
use crate::test_utils::db::{MemoryStore, StorageEntry};
use crate::test_utils::mocks::Client;

/// Mock node which simulates the functionality of a real node in the p2panda.
///
/// It contains an implementation of `StorageProvider` which exposes methods for publishing
/// and storing entries and operations and accessing materialised documents and their views.
///
/// Offers a sync interface to some of the underlying async `StorageProvider` methods.
#[derive(Debug, Default)]
pub struct Node(MemoryStore);

impl Node {
    /// Create a new mock Node.
    pub fn new() -> Self {
        Self(MemoryStore::default())
    }

    /// Return the entire store.
    pub fn store(&self) -> &MemoryStore {
        &self.0
    }

    /// Publish an entry to the node.
    ///
    /// This method is a sync wrapper around the equivalent async method on the storage
    /// provider. It validates and publishes an entry to the node. Additionally it seperately
    /// stores the contained operation and triggers materialisation of documents and views.
    ///
    /// Equivalent to using the helper method `send_to_store()` to publish entries.
    pub async fn publish_entry(
        &mut self,
        entry: &EncodedEntry,
        operation: &EncodedOperation,
    ) -> Result<PublishEntryResponse> {
        todo!()
        // TODO: refactore using `validate` and `domain` methods
    }

    /// Get the next entry arguments for an author and optionally existing document.
    pub async fn get_next_entry_args(
        &self,
        author: &Author,
        document_id: Option<&DocumentId>,
    ) -> Result<EntryArgsResponse> {
        todo!()
        // TODO: refactore using `validate` and `domain` methods
    }

    /// Get all entries stored on the node.
    pub fn entries(&self) -> HashMap<Hash, StorageEntry> {
        self.store().entries.lock().unwrap().clone()
    }

    /// Get all operations stored on the node.
    pub fn operations(&self) -> HashMap<OperationId, VerifiedOperation> {
        self.store()
            .operations
            .lock()
            .unwrap()
            .iter()
            .map(|(id, (_, operation))| (id.clone(), operation.clone()))
            .collect()
    }

    /// Get all documents stored on the node.
    pub fn documents(&self) -> HashMap<DocumentId, Document> {
        self.store().documents.lock().unwrap().clone()
    }

    /// Get all document views stored on the node.
    pub fn document_views(&self) -> HashMap<DocumentViewId, DocumentView> {
        self.store()
            .document_views
            .lock()
            .unwrap()
            .iter()
            .map(|(id, (_, document_view))| (id.clone(), document_view.clone()))
            .collect()
    }

    /// Get all authors who have published to this node.
    pub fn authors(&self) -> HashSet<Author> {
        let mut authors = HashSet::new();
        let entries = self.store().entries.lock().unwrap();
        for (_, entry) in entries.iter() {
            authors.insert(entry.public_key().to_owned());
        }
        authors
    }
}

/// Helper method for encoding, signing and sending operations to a node.
///
/// Internally it composes an entry to encode the operation on, first requesting the next
/// entry args from the node itself before signing it with the passed client and publishing.
///
/// Every call to this method also triggers the effected document to be re-materialised
/// and it's new state to be stored in preperation for answering query requests.
pub async fn send_to_node(
    node: &mut Node,
    client: &Client,
    operation: &Operation,
) -> Result<(Hash, PublishEntryResponse)> {
    // Insert the entry, operation and log into the database.
    let (entry_encoded, response) = send_to_store(&node.0, operation, &client.key_pair).await?;

    // Trigger materialisation by processing the new operation.
    process_new_operation(node, &entry_encoded.hash().into()).await?;

    Ok((entry_encoded.hash(), response))
}

/// Re-materialise the document effected by the passed operation.
///
/// Errors if a document for this operation does not already exist in the database.
pub async fn process_new_operation(node: &mut Node, operation: &OperationId) -> Result<()> {
    let document_id = node
        .0
        .get_document_by_entry(operation.as_hash())
        .await?
        .expect("No document found for operation");

    // Now we perform materialisation on the effected document.
    let document_operations = node.0.get_operations_by_document_id(&document_id).await?;
    let document = DocumentBuilder::new(document_operations).build()?;

    // This inserts the document and it's current view into the store.
    node.0.insert_document(&document).await?;
    Ok(())
}

// TODO: Needs reinstating when we deal with https://github.com/p2panda/p2panda/issues/418
// #[cfg(test)]
// mod tests {
//     use std::convert::TryFrom;
//
//     use rstest::rstest;
//
//     use crate::document::{DocumentId, DocumentViewId};
//     use crate::entry::{LogId, SeqNum};
//     use crate::identity::KeyPair;
//     use crate::operation::{EncodedOperation, OperationValue};
//     use crate::test_utils::db::EntryArgsResponse;
//     use crate::test_utils::fixtures::{
//         create_operation, delete_operation, key_pair, private_key, update_operation,
//     };
//     use crate::test_utils::mocks::client::Client;
//
//     use super::{send_to_node, Node};
//
//     #[rstest]
//     #[tokio::test]
//     async fn publishing_entries(private_key: String) {
//         let panda = Client::new("panda".to_string(), key_pair(&private_key));
//         let mut node = Node::new();
//
//         // This is an empty node which has no author logs.
//         let next_entry_args = node
//             .get_next_entry_args(&panda.author(), None)
//             .await
//             .unwrap();
//
//         // These are the next_entry_args we would expect to get when making a request to this node.
//         let mut expected_next_entry_args = EntryArgsResponse {
//             log_id: LogId::new(0),
//             seq_num: SeqNum::new(1).unwrap(),
//             backlink: None,
//             skiplink: None,
//         };
//
//         assert_eq!(next_entry_args, expected_next_entry_args);
//
//         // Panda publishes a create operation.
//         // This instantiates a new document.
//         //
//         // PANDA  : [1]
//         let (panda_entry_1_hash, next_entry_args) = send_to_node(
//             &mut node,
//             &panda,
//             &create_operation(&[(
//                 "message",
//                 OperationValue::Text("Ohh, my first message! [Panda]".to_string()),
//             )]),
//         )
//         .await
//         .unwrap();
//
//         // The document id is derived from the hash of it's first entry.
//         let document_id = panda_entry_1_hash.clone().into();
//
//         // The seq_num has incremented to 2 because panda already published one entry.
//         expected_next_entry_args = EntryArgsResponse {
//             log_id: LogId::new(0),
//             seq_num: SeqNum::new(2).unwrap(),
//             backlink: Some(panda_entry_1_hash.clone()),
//             skiplink: None,
//         };
//
//         assert_eq!(next_entry_args, expected_next_entry_args);
//
//         // The database contains one author now.
//         assert_eq!(node.authors().len(), 1);
//
//         // Panda publishes an update operation.
//         // It contains the hash of the current graph tip in it's `previous_operations`.
//         //
//         // PANDA  : [1] <-- [2]
//         let (panda_entry_2_hash, next_entry_args) = send_to_node(
//             &mut node,
//             &panda,
//             &update_operation(
//                 &[(
//                     "message",
//                     OperationValue::Text("Which I now update. [Panda]".to_string()),
//                 )],
//                 &panda_entry_1_hash.into(),
//             ),
//         )
//         .await
//         .unwrap();
//
//         expected_next_entry_args = EntryArgsResponse {
//             log_id: LogId::new(0),
//             seq_num: SeqNum::new(3).unwrap(),
//             backlink: Some(panda_entry_2_hash.clone()),
//             skiplink: None,
//         };
//
//         assert_eq!(next_entry_args, expected_next_entry_args);
//
//         assert_eq!(node.authors().len(), 1);
//
//         let penguin = Client::new("penguin".to_string(), KeyPair::new());
//
//         let next_entry_args = node
//             .get_next_entry_args(&penguin.author(), Some(&document_id))
//             .await
//             .unwrap();
//
//         expected_next_entry_args = EntryArgsResponse {
//             log_id: LogId::new(0),
//             seq_num: SeqNum::new(1).unwrap(),
//             backlink: None,
//             skiplink: None,
//         };
//
//         assert_eq!(next_entry_args, expected_next_entry_args);
//
//         // Penguin publishes an update operation which refers to panda's last operation
//         // as the graph tip.
//         //
//         // PANDA  : [1] <--[2]
//         // PENGUIN:           \--[1]
//         let (penguin_entry_1_hash, next_entry_args) = send_to_node(
//             &mut node,
//             &penguin,
//             &update_operation(
//                 &[(
//                     "message",
//                     OperationValue::Text("My turn to update. [Penguin]".to_string()),
//                 )],
//                 &panda_entry_2_hash.into(),
//             ),
//         )
//         .await
//         .unwrap();
//
//         expected_next_entry_args = EntryArgsResponse {
//             log_id: LogId::new(0),
//             seq_num: SeqNum::new(2).unwrap(),
//             backlink: Some(penguin_entry_1_hash.clone()),
//             skiplink: None,
//         };
//
//         assert_eq!(next_entry_args, expected_next_entry_args);
//
//         assert_eq!(node.authors().len(), 2);
//
//         // Penguin publishes another update operation refering to their own previous operation
//         // as the graph tip.
//         //
//         // PANDA  : [1] <--[2]
//         // PENGUIN:           \--[1] <--[2]
//         let (penguin_entry_2_hash, next_entry_args) = send_to_node(
//             &mut node,
//             &penguin,
//             &update_operation(
//                 &[(
//                     "message",
//                     OperationValue::Text("And again. [Penguin]".to_string()),
//                 )],
//                 &penguin_entry_1_hash.into(),
//             ),
//         )
//         .await
//         .unwrap();
//
//         expected_next_entry_args = EntryArgsResponse {
//             log_id: LogId::new(0),
//             seq_num: SeqNum::new(3).unwrap(),
//             backlink: Some(penguin_entry_2_hash),
//             skiplink: None,
//         };
//
//         assert_eq!(next_entry_args, expected_next_entry_args);
//
//         // Now there are 2 authors publishing ot the node.
//         assert_eq!(node.authors().len(), 2);
//
//         // We can query the node for the current document state.
//         let document = node.documents().get(&document_id).unwrap().clone();
//         let document_view_value = document.view().unwrap().get("message").unwrap();
//         // It was last updated by Penguin, this writes over previous values.
//         assert_eq!(
//             document_view_value.value(),
//             &OperationValue::Text("And again. [Penguin]".to_string())
//         );
//         // There should only be one document in the database.
//         assert_eq!(node.documents().len(), 1);
//
//         // Panda publishes another create operation.
//         // This again instantiates a new document.
//         //
//         // PANDA  : [1]
//         let (panda_entry_1_hash, next_entry_args) = send_to_node(
//             &mut node,
//             &panda,
//             &create_operation(&[(
//                 "message",
//                 OperationValue::Text("Ohh, my first message in a new document!".to_string()),
//             )]),
//         )
//         .await
//         .unwrap();
//
//         expected_next_entry_args = EntryArgsResponse {
//             log_id: LogId::new(1),
//             seq_num: SeqNum::new(2).unwrap(),
//             backlink: Some(panda_entry_1_hash),
//             skiplink: None,
//         };
//
//         assert_eq!(next_entry_args, expected_next_entry_args);
//
//         assert_eq!(node.authors().len(), 2);
//         // There should be 2 document in the database.
//         assert_eq!(node.documents().len(), 2);
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn concurrent_updates(private_key: String) {
//         let panda = Client::new("panda".to_string(), key_pair(&private_key));
//         let penguin = Client::new(
//             "penguin".to_string(),
//             key_pair("eb852fefa703901e42f17cdc2aa507947f392a72101b2c1a6d30023af14f75e3"),
//         );
//         let mut node = Node::new();
//
//         // Publish a CREATE operation
//         //
//         // PANDA  : [1]
//         let (panda_entry_1_hash, _) = send_to_node(
//             &mut node,
//             &panda,
//             &create_operation(&[
//                 (
//                     "cafe_name",
//                     OperationValue::Text("Polar Pear Cafe".to_string()),
//                 ),
//                 (
//                     "address",
//                     OperationValue::Text("1, Polar Bear Rise, Panda Town".to_string()),
//                 ),
//             ]),
//         )
//         .await
//         .unwrap();
//
//         let document_id = panda_entry_1_hash.clone().into();
//
//         let document = node.documents().get(&document_id).unwrap().to_owned();
//         let document_view_value = document.view().unwrap().get("cafe_name").unwrap();
//         assert_eq!(
//             document_view_value.value(),
//             &OperationValue::Text("Polar Pear Cafe".to_string())
//         );
//
//         // Publish an UPDATE operation
//         //
//         // PANDA  : [1] <--[2]
//         let (panda_entry_2_hash, _) = send_to_node(
//             &mut node,
//             &panda,
//             &update_operation(
//                 &[(
//                     "cafe_name",
//                     OperationValue::Text("Polar Bear Cafe".to_string()),
//                 )],
//                 &panda_entry_1_hash.clone().into(),
//             ),
//         )
//         .await
//         .unwrap();
//
//         let document = node.documents().get(&document_id).unwrap().to_owned();
//         let document_view_value = document.view().unwrap().get("cafe_name").unwrap();
//         assert_eq!(
//             document_view_value.value(),
//             &OperationValue::Text("Polar Bear Cafe".to_string())
//         );
//
//         // Penguin publishes an UPDATE operation, but they haven't seen Panda's most recent entry [2]
//         // making this a concurrent update which forks the document graph.
//         //
//         // PANDA  : [1] <--[2]
//         //            \
//         // PENGUIN:    [1]
//         let (penguin_entry_1_hash, _) = send_to_node(
//             &mut node,
//             &penguin,
//             &update_operation(
//                 &[(
//                     "address",
//                     OperationValue::Text("1, Polar Bear rd, Panda Town".to_string()),
//                 )],
//                 &panda_entry_1_hash.into(),
//             ),
//         )
//         .await
//         .unwrap();
//
//         let document = node.documents().get(&document_id).unwrap().to_owned();
//         let document_view_value = document.view().unwrap().get("cafe_name").unwrap();
//         assert_eq!(
//             document_view_value.value(),
//             &OperationValue::Text("Polar Bear Cafe".to_string())
//         );
//
//         // Penguin publishes another UPDATE operation, this time they have replicated all entries
//         // and refer to the two existing document graph tips in the previous_operation fields.
//         //
//         // PANDA  : [1] <-- [2]
//         //            \        \
//         // PENGUIN:    [1] <-- [2]
//         let (_penguin_entry_2_hash, _) = send_to_node(
//             &mut node,
//             &penguin,
//             &update_operation(
//                 &[(
//                     "cafe_name",
//                     OperationValue::Text("Polar Bear Café".to_string()),
//                 )],
//                 &DocumentViewId::new(&[penguin_entry_1_hash.into(), panda_entry_2_hash.into()])
//                     .unwrap(),
//             ),
//         )
//         .await
//         .unwrap();
//
//         let document = node.documents().get(&document_id).unwrap().clone();
//         let document_view_value = document.view().unwrap().get("cafe_name").unwrap();
//         assert_eq!(
//             document_view_value.value(),
//             &OperationValue::Text("Polar Bear Café".to_string())
//         );
//
//         // As more operations are published, the graph could look like this:
//         //
//         // PANDA  : [1] <--[2]          [3] <--[4] <--[5]
//         //            \       \         /
//         // PENGUIN:    [1] <--[2] <--[3]
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn publish_many_entries() {
//         let client = Client::new("panda".into(), KeyPair::new());
//         let num_of_entries = 50;
//
//         let mut node_1 = Node::new();
//         let mut node_2 = Node::new();
//
//         let mut document_id: Option<DocumentId> = None;
//
//         for seq_num in 1..num_of_entries + 1 {
//             let entry_args = node_1
//                 .get_next_entry_args(&client.author(), document_id.as_ref())
//                 .await
//                 .unwrap();
//
//             let operation = if seq_num == 1 {
//                 create_operation(&[("name", OperationValue::Text("Panda".to_string()))])
//             } else if seq_num == (num_of_entries + 1) {
//                 delete_operation(&entry_args.backlink.clone().unwrap().into())
//             } else {
//                 update_operation(
//                     &[("name", OperationValue::Text("🐼".to_string()))],
//                     &entry_args.backlink.clone().unwrap().into(),
//                 )
//             };
//
//             // Send the entry to node_1 using `send_to_node()`
//             let result = send_to_node(&mut node_1, &client, &operation).await;
//             assert!(result.is_ok());
//
//             // Send the entry to node_2 using `node.publish_entry()`
//             let entry = client.signed_encoded_entry(
//                 operation.clone(),
//                 &entry_args.log_id,
//                 entry_args.skiplink.as_ref(),
//                 entry_args.backlink.as_ref(),
//                 &entry_args.seq_num,
//             );
//
//             let encoded_operation = EncodedOperation::try_from(&operation).unwrap();
//
//             let result = node_2.publish_entry(&entry, &encoded_operation).await;
//             assert!(result.is_ok());
//
//             // Set the document id if this was the first entry
//             if seq_num == 1 {
//                 document_id = Some(entry.hash().into());
//             }
//         }
//
//         assert_eq!(node_1.0.entries.lock().unwrap().len(), 50);
//         assert_eq!(node_1.0.logs.lock().unwrap().len(), 1);
//         assert_eq!(node_1.0.documents.lock().unwrap().len(), 1);
//         assert_eq!(node_1.0.document_views.lock().unwrap().len(), 50);
//         assert_eq!(node_2.0.entries.lock().unwrap().len(), 50);
//         assert_eq!(node_2.0.logs.lock().unwrap().len(), 1);
//         assert_eq!(node_2.0.documents.lock().unwrap().len(), 1);
//         assert_eq!(node_2.0.document_views.lock().unwrap().len(), 50);
//     }
// }
