// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::DocumentBuilderError;
use crate::hash::Hash;
use crate::instance::Instance;
use crate::materialiser::Graph;
use crate::operation::{AsOperation, OperationWithMeta};

/// A resolvable data type made up of a collection of causally linked operations.
///
/// Implements the `Resolve` trait for every different schema we support.
#[derive(Debug, Clone)]
pub struct Document {
    id: Hash,
    schema: Hash,
    view: Instance,
    operations: Vec<OperationWithMeta>,
}

impl Document {
    /// Static method for resolving this document into a single view.
    fn resolve_view(operations: &[OperationWithMeta]) -> Result<Instance, DocumentBuilderError> {
        // Instantiate graph and operations map.
        let mut graph = Graph::new();

        // Add all operations to the graph.
        for operation in operations {
            graph.add_node(operation.operation_id().as_str(), operation.clone());
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
        let mut sorted_operations = graph.sort()?.into_iter();

        // Instantiate an initial docuent view from the documents create operation.
        //
        // We can unwrap here because we already verified the operations during the document building
        // which means we know there is at least one CREATE operation.
        let mut document_view = Instance::try_from(sorted_operations.next().unwrap())?;

        // Apply every update in order to arrive at the current view.
        sorted_operations.try_for_each(|op| document_view.apply_update(op))?;

        Ok(document_view)
    }
}

impl Document {
    /// Get the view of this document.
    pub fn view(&self) -> &Instance {
        &self.view
    }

    /// Get the operations contianed in this document.
    pub fn operations(&self) -> &Vec<OperationWithMeta> {
        &self.operations
    }

    /// Get the document id.
    pub fn id(&self) -> &Hash {
        &self.id
    }

    /// Get the document schema.
    pub fn schema(&self) -> &Hash {
        &self.schema
    }

    // More nice methods....
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
    pub fn build(&self) -> Result<Document, DocumentBuilderError> {
        // Validate the operation collection contained in this document.
        let (id, schema) = self.validate()?;

        let view = Document::resolve_view(&self.operations)?;

        Ok(Document {
            id,
            schema,
            view,
            operations: self.operations(),
        })
    }

    /// Validate the collection of operations which are contained in this document.
    /// - there should be exactly one CREATE operation.
    /// - all operations should follow the same schema.
    pub fn validate(&self) -> Result<(Hash, Hash), DocumentBuilderError> {
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

        // Get the she document schema
        let document_schema = create_operation.schema();

        // Check all operations match the document schema
        let schema_error = self
            .operations()
            .iter()
            .any(|operation| operation.schema() != document_schema);

        if schema_error {
            return Err(DocumentBuilderError::OperationSchemaNotMatching);
        }

        let document_id = create_operation.operation_id().to_owned();

        Ok((document_id, document_schema))
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
        let (penguin_entry_1_hash, _) = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                vec![panda_entry_1_hash],
                fields(vec![(
                    "name",
                    OperationValue::Text("Penguin Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous_operations` field now contains 2 operation hash id's.
        let (penguin_entry_2_hash, _) = send_to_node(
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

        // // Document should resolve to expected value
        assert_eq!(document.unwrap().view().get("name"), exp_result.get("name"));

        // Multiple replicas receiving operations in different orders should resolve to same value.

        let op_1 = operations.get(0).unwrap();
        let op_2 = operations.get(1).unwrap();
        let op_3 = operations.get(2).unwrap();
        let op_4 = operations.get(3).unwrap();
        let op_5 = operations.get(4).unwrap();

        let replica_1 = DocumentBuilder::new(vec![
            op_5.clone(),
            op_4.clone(),
            op_3.clone(),
            op_2.clone(),
            op_1.clone(),
        ])
        .build()
        .unwrap();

        let replica_2 = DocumentBuilder::new(vec![
            op_3.clone(),
            op_2.clone(),
            op_1.clone(),
            op_5.clone(),
            op_4.clone(),
        ])
        .build()
        .unwrap();

        let replica_3 = DocumentBuilder::new(vec![
            op_2.clone(),
            op_1.clone(),
            op_4.clone(),
            op_3.clone(),
            op_5.clone(),
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
