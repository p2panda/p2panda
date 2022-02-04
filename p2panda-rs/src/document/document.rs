// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{DocumentBuilderError, DocumentView};
use crate::graph::Graph;
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, OperationWithMeta};

/// A resolvable data type made up of a collection of causally linked operations.
///
/// Implements the `Resolve` trait for every different schema we support.
#[derive(Debug, Clone)]
pub struct Document {
    id: Hash,
    author: Author,
    schema: Hash,
    view: DocumentView,
    meta: DocumentMeta,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentMeta {
    deleted: bool,
    edited: bool,
    operations: Vec<OperationWithMeta>,
    current_graph_tips: Vec<Hash>,
}

impl Document {
    /// Static method for resolving this document into a single view.
    fn resolve_view(
        operations: &[OperationWithMeta],
        meta: &mut DocumentMeta,
    ) -> Result<DocumentView, DocumentBuilderError> {
        // Instantiate graph and operations map.
        let mut graph = Graph::new();

        if operations.len() > 1 {
            meta.edited = true
        }

        // Add all operations to the graph.
        for operation in operations {
            graph.add_node(operation.operation_id().as_str(), operation.clone());
            if operation.is_delete() {
                meta.deleted = true
            }
        }

        // Add links between operations in the graph.
        for operation in operations {
            if let Some(previous_operations) = operation.previous_operations() {
                for previous in previous_operations {
                    let success =
                        graph.add_link(previous.as_str(), operation.operation_id().as_str());
                    if !success {
                        return Err(DocumentBuilderError::InvalidOperationLink(
                            operation.operation_id().as_str().into(),
                        ));
                    }
                }
            }
        }

        // Traverse the graph topologically and return an ordered list of operations.
        let sorted_graph_data = graph.sort()?;

        // Instantiate an initial document view from the documents create operation.
        //
        // We can unwrap here because we already verified the operations during the document building
        // which means we know there is at least one CREATE operation.
        let mut operations_iter = sorted_graph_data.sorted().into_iter();
        let mut document_view = DocumentView::try_from(operations_iter.next().unwrap())?;

        // Apply every update in order to arrive at the current view.
        operations_iter.try_for_each(|op| document_view.apply_update(op))?;

        // Populate document meta data fields.
        meta.operations = sorted_graph_data.sorted();
        meta.current_graph_tips = sorted_graph_data
            .current_graph_tips()
            .iter()
            .map(|operation| operation.operation_id().to_owned())
            .collect();

        Ok(document_view)
    }
}

impl Document {
    /// Get the document id.
    pub fn id(&self) -> &Hash {
        &self.id
    }

    /// Get the document author.
    pub fn author(&self) -> &Author {
        &self.author
    }

    /// Get the document schema.
    pub fn schema(&self) -> &Hash {
        &self.schema
    }

    /// Get the view of this document.
    pub fn view(&self) -> &DocumentView {
        &self.view
    }

    /// Get the operations contianed in this document.
    pub fn operations(&self) -> &Vec<OperationWithMeta> {
        &self.meta.operations
    }

    /// Get the documents graph tips.
    pub fn current_graph_tips(&self) -> &Vec<Hash> {
        &self.meta.current_graph_tips
    }

    /// Returns true if this document has applied an UPDATE operation.
    pub fn is_edited(&self) -> bool {
        self.meta.edited
    }

    /// Returns true if this document has processed a DELETE operation.
    pub fn is_deleted(&self) -> bool {
        self.meta.deleted
    }
}

/// A struct for building documents.
#[derive(Debug, Clone)]
pub struct DocumentBuilder {
    operations: Vec<OperationWithMeta>,
}

impl DocumentBuilder {
    /// Instantiate a new DocumentBuilder with a collection of operations.
    pub fn new(operations: Vec<OperationWithMeta>) -> DocumentBuilder {
        Self { operations }
    }

    /// Get all operations for this document.
    pub fn operations(&self) -> Vec<OperationWithMeta> {
        self.operations.clone()
    }

    /// Build document. This already resolves the current document view.
    /// Validate the collection of operations which are contained in this document.
    /// - there should be exactly one CREATE operation.
    /// - all operations should follow the same schema.

    pub fn build(&self) -> Result<Document, DocumentBuilderError> {
        // find create message.
        let mut collect_create_operation: Vec<OperationWithMeta> = self
            .operations()
            .into_iter()
            .filter(|op| op.is_create())
            .collect();

        // Check we have only one create operation in the document.
        let create_operation = match collect_create_operation.len() {
            0 => Err(DocumentBuilderError::NoCreateOperation),
            1 => Ok(collect_create_operation.pop().unwrap()),
            _ => Err(DocumentBuilderError::MoreThanOneCreateOperation),
        }?;

        // Get the document schema
        let schema = create_operation.schema();

        // Get the document author (or rather, the public key of the author who created this document)
        let author = create_operation.public_key().to_owned();

        // Check all operations match the document schema
        let schema_error = self
            .operations()
            .iter()
            .any(|operation| operation.schema() != schema);

        if schema_error {
            return Err(DocumentBuilderError::OperationSchemaNotMatching);
        }

        let id = create_operation.operation_id().to_owned();

        let mut meta = DocumentMeta {
            operations: self.operations(),
            ..Default::default()
        };

        let view = Document::resolve_view(&self.operations, &mut meta)?;

        Ok(Document {
            id,
            schema,
            author,
            view,
            meta,
        })
    }
}

// @TODO: This currently makes sure the wasm tests work as cddl does not have any wasm support
// (yet). Remove this with: https://github.com/p2panda/p2panda/issues/99
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use rstest::rstest;
    use std::collections::BTreeMap;

    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::{OperationValue, OperationWithMeta};
    use crate::test_utils::fixtures::{
        create_operation, fields, random_key_pair, schema, update_operation,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    use super::DocumentBuilder;

    #[rstest]
    fn resolve_documents(schema: Hash) {
        let panda = Client::new(
            "panda".to_string(),
            KeyPair::from_private_key_str(
                "ddcafe34db2625af34c8ba3cf35d46e23283d908c9848c8b43d1f5d0fde779ea",
            )
            .unwrap(),
        );
        let penguin = Client::new(
            "penguin".to_string(),
            KeyPair::from_private_key_str(
                "1c86b2524b48f0ba86103cddc6bdfd87774ab77ab4c0ea989ed0eeab3d28827a",
            )
            .unwrap(),
        );
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        //
        // DOCUMENT: [panda_1]
        //
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Panda publishes an update operation.
        // It contains the hash of the previous operation in it's `previous_operations` array
        //
        // DOCUMENT: [panda_1]<--[panda_2]
        //
        let (panda_entry_2_hash, _) = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema.clone(),
                vec![panda_entry_1_hash.clone()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]
        //                    \----[panda_2]
        let (penguin_entry_1_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                vec![panda_entry_1_hash.clone()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Penguin Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous_operations` field now contains 2 operation hash id's.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]
        //                    \----[panda_2]<--/
        let (penguin_entry_2_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                vec![penguin_entry_1_hash.clone(), panda_entry_2_hash.clone()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Penguin publishes a new update operation which points at the current graph tip.
        //
        // DOCUMENT: [panda_1]<--[penguin_1]<---[penguin_2]<--[penguin_3]
        //                    \----[panda_2]<--/
        let (penguin_entry_3_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema,
                vec![penguin_entry_2_hash.clone()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
                )]),
            ),
        )
        .unwrap();

        let operations: Vec<OperationWithMeta> = node
            .all_entries()
            .into_iter()
            .map(|entry| {
                OperationWithMeta::new(&entry.entry_encoded(), &entry.operation_encoded()).unwrap()
            })
            .collect();

        let document = DocumentBuilder::new(operations.clone()).build();

        assert!(document.is_ok());

        let mut exp_result = BTreeMap::new();
        exp_result.insert(
            "name".to_string(),
            OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
        );

        let panda_1 = operations
            .iter()
            .find(|op| op.operation_id() == &panda_entry_1_hash)
            .unwrap();
        let panda_2 = operations
            .iter()
            .find(|op| op.operation_id() == &panda_entry_2_hash)
            .unwrap();
        let penguin_1 = operations
            .iter()
            .find(|op| op.operation_id() == &penguin_entry_1_hash)
            .unwrap();
        let penguin_2 = operations
            .iter()
            .find(|op| op.operation_id() == &penguin_entry_2_hash)
            .unwrap();
        let penguin_3 = operations
            .iter()
            .find(|op| op.operation_id() == &penguin_entry_3_hash)
            .unwrap();

        let expected_graph_tip = vec![penguin_entry_3_hash.clone()];
        let expected_op_order = vec![
            panda_1.to_owned(),
            panda_2.to_owned(),
            penguin_1.to_owned(),
            penguin_2.to_owned(),
            penguin_3.to_owned(),
        ];

        // // Document should resolve to expected value

        let document = document.unwrap();
        assert_eq!(document.view().get("name"), exp_result.get("name"));
        assert!(document.is_edited());
        assert!(!document.is_deleted());
        assert_eq!(document.operations(), &expected_op_order);
        assert_eq!(document.current_graph_tips(), &expected_graph_tip);

        // Multiple replicas receiving operations in different orders should resolve to same value.

        let replica_1 = DocumentBuilder::new(vec![
            penguin_2.clone(),
            penguin_1.clone(),
            penguin_3.clone(),
            panda_2.clone(),
            panda_1.clone(),
        ])
        .build()
        .unwrap();

        let replica_2 = DocumentBuilder::new(vec![
            penguin_3.clone(),
            panda_2.clone(),
            panda_1.clone(),
            penguin_2.clone(),
            penguin_1.clone(),
        ])
        .build()
        .unwrap();

        let replica_3 = DocumentBuilder::new(vec![
            panda_2.clone(),
            panda_1.clone(),
            penguin_1.clone(),
            penguin_3.clone(),
            penguin_2.clone(),
        ])
        .build()
        .unwrap();

        assert_eq!(replica_1.view().get("name"), replica_2.view().get("name"));
        assert_eq!(replica_1.view().get("name"), replica_3.view().get("name"));
    }

    #[rstest]
    fn must_have_create_operation(schema: Hash, #[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let (panda_entry_1_hash, _) = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Panda publishes an update operation.
        // It contains the hash of the previous operation in it's `previous_operations` array
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema,
                vec![panda_entry_1_hash],
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Only retrieve the update operation.
        let only_the_update_operation = &node.all_entries()[1];

        let operations = vec![OperationWithMeta::new(
            &only_the_update_operation.entry_encoded(),
            &only_the_update_operation.operation_encoded(),
        )
        .unwrap()];

        assert!(DocumentBuilder::new(operations).build().is_err());
    }
}
