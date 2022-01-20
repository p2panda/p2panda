// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

// // Now we use our own graph for topo sorting :-)
// use incremental_topo::IncrementalTopo;

use crate::document::{DocumentBuilderError, DocumentError};
use crate::hash::Hash;
use crate::identity::Author;
use crate::instance::Instance;
use crate::materialiser::Graph;
use crate::operation::{AsOperation, OperationWithMeta};
use crate::schema::{Schema, ValidateOperation};

/// Hard coded cddl string for now
const DOCUMENT_SCHEMA: &str = "cafe = { (
    name: { type: \"str\", value: tstr }
) }";

/// An iterator struct for Document.
#[derive(Debug)]
pub struct DocumentIter(Vec<OperationWithMeta>);

/// A resolvable data type made up of a collection of causally linked operations.
#[derive(Debug, Clone)]
pub struct Document {
    /// The create operation which is the root of this document.
    create_operation: OperationWithMeta,
    /// A causal graph of this documents operations which can be topologically sorted.
    graph: Graph<OperationWithMeta>,
}

impl Document {
    /// The hash id of this document.
    pub fn id(&self) -> Hash {
        self.create_operation.operation_id().to_owned()
    }

    /// The hash id of this documents schema.
    pub fn schema(&self) -> Hash {
        self.create_operation.schema()
    }

    /// The author of this document.
    pub fn author(&self) -> Author {
        self.create_operation.public_key().to_owned()
    }

    /// Returns an iterator over all operations in this document ordered topologically.
    pub fn iter(&self) -> Result<DocumentIter, DocumentError> {
        let sorted = match self.graph.sort() {
            Ok(operations) => Ok(operations.into_iter().rev().collect()),
            Err(e) => Err(DocumentError::GraphSortingError(e)),
        }?;

        Ok(DocumentIter(sorted))
    }

    /// Sort the graph topologically, then reduce the linearised operations into a single
    /// `Instance`.
    pub fn resolve(&self) -> Result<Instance, DocumentError> {
        let mut document_iter = self.iter()?;

        let create_message = document_iter.next().unwrap();
        let mut instance = Instance::try_from(create_message)?;

        document_iter.try_for_each(|op| instance.apply_update(op))?;

        Ok(instance)
    }
}

impl Iterator for DocumentIter {
    type Item = OperationWithMeta;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.pop()
    }
}

/// A struct for building documents.
#[derive(Debug)]
pub struct DocumentBuilder {
    /// An unsorted collection of operations which are associated with a particular document id.
    operations: Vec<OperationWithMeta>,
    schema_definition: String,
}

impl DocumentBuilder {
    /// Instantiate a new DocumentBuilder with a collection of operations.
    pub fn new(operations: Vec<OperationWithMeta>, schema_definition: String) -> Self {
        Self {
            operations,
            schema_definition,
        }
    }

    /// Get all operations for this document.
    pub fn operations(&self) -> Vec<OperationWithMeta> {
        self.operations.clone()
    }

    /// Get an iterator over all operations in this document.
    pub fn operations_iter(&self) -> std::vec::IntoIter<OperationWithMeta> {
        self.operations.clone().into_iter()
    }

    /// Build the document.
    pub fn build(self) -> Result<Document, DocumentBuilderError> {
        // find create message.
        let collect_create_operation: Vec<OperationWithMeta> =
            self.operations_iter().filter(|op| op.is_create()).collect();

        // Check we have only one create operation in the document.
        if collect_create_operation.len() > 1 {
            return Err(DocumentBuilderError::MoreThanOneCreateOperation);
        } else if collect_create_operation.is_empty() {
            return Err(DocumentBuilderError::NoCreateOperation);
        }

        let create_operation = &collect_create_operation[0];

        let document_schema = create_operation.schema();

        // Create instantiate a schema, which validates the provided schema's CDDL definition.
        let schema = Schema::new(&create_operation.schema(), &self.schema_definition)?;

        // Validate the create message for this document against the document schema.
        // @TODO: Exclude from wasm target until cddl issues sorted (https://github.com/p2panda/p2panda/issues/99).
        #[cfg(not(target_arch = "wasm32"))]
        schema.validate_operation_fields(&create_operation.fields().unwrap())?;

        // Instantiate graph and operations map.
        let mut graph = Graph::new();

        // Add all operations to the graph.
        for operation in self.operations() {
            // Validate all operations refer to the same document schema.
            if operation.schema() != document_schema {
                return Err(DocumentBuilderError::OperationSchemaNotMatching(
                    operation.operation_id().as_str().into(),
                ));
            }
            graph.add_node(operation.operation_id().as_str(), operation.clone());
        }

        // Add links between operations in the graph.
        for operation in self.operations() {
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

        Ok(Document {
            create_operation: create_operation.to_owned(),
            graph,
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

    use super::{DocumentBuilder, DOCUMENT_SCHEMA};

    #[rstest]
    fn sort_and_resolve_graph(
        schema: Hash,
        #[from(random_key_pair)] key_pair_1: KeyPair,
        #[from(random_key_pair)] key_pair_2: KeyPair,
    ) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let penguin = Client::new("penguin".to_string(), key_pair_2);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let panda_entry_1_hash = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
            None,
        )
        .unwrap();

        // Panda publishes an update operation.
        // It contains the hash of the previous operation in it's `previous_operations` array
        let panda_entry_2_hash = send_to_node(
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
            Some(&panda_entry_1_hash.clone()),
        )
        .unwrap();

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        let penguin_entry_1_hash = send_to_node(
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
            Some(&panda_entry_1_hash.clone()),
        )
        .unwrap();

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous_operations` field now contains 2 operation hash id's.
        let penguin_entry_2_hash = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                vec![penguin_entry_1_hash, panda_entry_2_hash],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe".to_string()),
                )]),
            ),
            Some(&panda_entry_1_hash),
        )
        .unwrap();

        // Penguin publishes a new update operation which points at the current graph tip.
        send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema,
                vec![penguin_entry_2_hash],
                fields(vec![(
                    "name",
                    OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
                )]),
            ),
            Some(&panda_entry_1_hash.clone()),
        )
        .unwrap();

        let operations: Vec<OperationWithMeta> = node
            .all_entries()
            .into_iter()
            .map(|entry| {
                OperationWithMeta::new(&entry.entry_encoded(), &entry.operation_encoded()).unwrap()
            })
            .collect();

        let document = DocumentBuilder::new(operations.clone(), DOCUMENT_SCHEMA.to_owned())
            .build()
            .unwrap();

        let instance = document.resolve().unwrap();

        let mut exp_result = BTreeMap::new();
        exp_result.insert(
            "name".to_string(),
            OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
        );

        // Document should resolve to expected value
        assert_eq!(instance.raw(), exp_result);

        // Multiple replicas receiving operations in different orders should resolve to same value.

        let op_1 = operations.get(0).unwrap();
        let op_2 = operations.get(1).unwrap();
        let op_3 = operations.get(2).unwrap();
        let op_4 = operations.get(3).unwrap();
        let op_5 = operations.get(4).unwrap();

        let replica_1 = DocumentBuilder::new(
            vec![
                op_5.clone(),
                op_4.clone(),
                op_3.clone(),
                op_2.clone(),
                op_1.clone(),
            ],
            DOCUMENT_SCHEMA.to_owned(),
        )
        .build()
        .unwrap();

        let replica_2 = DocumentBuilder::new(
            vec![
                op_3.clone(),
                op_2.clone(),
                op_1.clone(),
                op_5.clone(),
                op_4.clone(),
            ],
            DOCUMENT_SCHEMA.to_owned(),
        )
        .build()
        .unwrap();

        let replica_3 = DocumentBuilder::new(
            vec![
                op_2.clone(),
                op_1.clone(),
                op_4.clone(),
                op_3.clone(),
                op_5.clone(),
            ],
            DOCUMENT_SCHEMA.to_owned(),
        )
        .build()
        .unwrap();

        assert_eq!(replica_1.resolve().unwrap(), replica_2.resolve().unwrap());
        assert_eq!(replica_1.resolve().unwrap(), replica_3.resolve().unwrap());
    }

    #[rstest]
    fn must_have_create_operation(schema: Hash, #[from(random_key_pair)] key_pair_1: KeyPair) {
        let panda = Client::new("panda".to_string(), key_pair_1);
        let mut node = Node::new();

        // Panda publishes a create operation.
        // This instantiates a new document.
        let panda_entry_1_hash = send_to_node(
            &mut node,
            &panda,
            &create_operation(
                schema.clone(),
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
            None,
        )
        .unwrap();

        // Panda publishes an update operation.
        // It contains the hash of the previous operation in it's `previous_operations` array
        send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema,
                vec![panda_entry_1_hash.clone()],
                fields(vec![(
                    "name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )]),
            ),
            Some(&panda_entry_1_hash),
        )
        .unwrap();

        // Only retrieve the update operation.
        let only_the_update_operation = &node.all_entries()[1];

        let operations = vec![OperationWithMeta::new(
            &only_the_update_operation.entry_encoded(),
            &only_the_update_operation.operation_encoded(),
        )
        .unwrap()];

        // Building a Document without a create operation should fail.
        assert!(DocumentBuilder::new(operations, DOCUMENT_SCHEMA.to_owned())
            .build()
            .is_err());
    }
}
