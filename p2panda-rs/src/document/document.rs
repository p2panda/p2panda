// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::convert::TryFrom;

use incremental_topo::IncrementalTopo;

use crate::document::{DocumentBuilderError, DocumentError};
use crate::hash::Hash;
use crate::identity::Author;
use crate::instance::Instance;
use crate::operation::{AsOperation, OperationWithMeta};
use crate::schema::{Schema, ValidateOperation};
use crate::Validate;

/// Hard coded cddl string for now
const DOCUMENT_SCHEMA: &str = "cafe = { (
    name: { type: \"str\", value: tstr }
) }";

/// An iterator struct for Document.
#[derive(Debug)]
pub struct DocumentIter(Vec<OperationWithMeta>);

/// A resolvable data type made up of a collection of causally linked operations.
#[derive(Debug)]
pub struct Document {
    /// The hash id of this document, it is the hash of the entry of this documents root CREATE operation.
    id: Hash,
    /// The hash id of the schema operations in this document follow.
    schema: Schema,
    /// The author (public key) who published the CREATE message which instantiated this document.
    author: Author,
    /// A map of all operations contained within this document.
    operations: BTreeMap<String, OperationWithMeta>,
    /// A causal graph of this documents operations which can be topologically sorted.
    graph: IncrementalTopo<OperationWithMeta>,
}

impl Document {
    /// The hash id of this document.
    pub fn id(&self) -> Hash {
        self.id.clone()
    }

    /// The hash id of this documents schema.
    pub fn schema(&self) -> Hash {
        self.schema.schema_hash()
    }

    /// The author of this document.
    pub fn author(&self) -> Author {
        self.author.clone()
    }

    /// Returns a map of all operations in this document.
    pub fn operations(&self) -> BTreeMap<String, OperationWithMeta> {
        self.operations.clone()
    }

    /// Returns an iterator over all operations in this document ordered topologically.
    pub fn iter(&self) -> Result<DocumentIter, DocumentError> {
        let create_operation = self.get_create_operation();
        let mut iter = vec![create_operation.clone()];
        let sorted = match self.graph.descendants(&create_operation) {
            Ok(descendants) => Ok(descendants),
            Err(_) => Err(DocumentError::IncrementalTopoError),
        }?;

        for op in sorted {
            iter.insert(0, op.to_owned())
        }

        Ok(DocumentIter(iter))
    }

    /// Get the create operation for this document. We unwrap and panic if the value is None
    /// as all documents should contain at least a create operation identified by this documents id.
    /// This was validated when building with DocumentBuilder.
    fn get_create_operation(&self) -> OperationWithMeta {
        self.operations
            .get(self.id().as_str())
            .expect("There should be a CREATE operation")
            .to_owned()
    }

    /// Sort the graph topologically, then reduce the linearised operations into a single
    /// `Instance`.
    pub fn resolve(&self) -> Result<Instance, DocumentError> {
        let mut document_iter = self.iter()?;

        let create_message = document_iter
            .next()
            .expect("There should be a CREATE operation");
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

#[cfg(not(target_arch = "wasm32"))]
impl Validate for Document {
    type Error = DocumentError;

    fn validate(&self) -> Result<(), Self::Error> {
        // NB. This validation is quite excessive as it's normally not possible to get to this
        // point while having broken some of these basic data restraints.

        // There must be a CREATE operation matching the document_id
        let create_operation = self.get_create_operation();
        if !create_operation.is_create() {
            return Err(DocumentError::ValidationError(
                "All documents must contain a CREATE operation identified by the document_id"
                    .to_string(),
            ));
        }

        // Validate each operation in this document.
        self.operations().iter().try_for_each(|(_, op)| {
            // If this is a delete operation check there are no fields.
            if op.is_delete() {
                if op.fields().is_none() {
                    return Ok(())
                } else {
                    return Err(DocumentError::ValidationError("DELETE operations should not contain any fields".to_string()))
                };
            };
            // Validate each create and update operation against the document schema.
            match self.schema.validate_operation_fields(&op.fields().unwrap()) {
                Ok(_) => Ok(()),
                Err(_) => Err(DocumentError::ValidationError(
                    "All CREATE and UPDATE operations in document must follow the schema description".to_string(),
                )),
            }
        })?;

        Ok(())
    }
}

/// A struct for building documents.
#[derive(Debug)]
pub struct DocumentBuilder {
    /// An unsorted collection of operations which are associated with a particular document id.
    operations: Vec<OperationWithMeta>,
}

impl DocumentBuilder {
    /// Instantiate a new DocumentBuilder with a collection of operations.
    pub fn new(mut operations: Vec<OperationWithMeta>) -> Self {
        // Sort operations alphabetically by their hash id.
        // This is important for graph building and sorting, operations must be added
        // to a IncrementalTopo graph in the correct (alphabetical) order to assure consistent
        // traversal.
        operations.sort_by(|a, b| a.operation_id().as_str().cmp(b.operation_id().as_str()));
        Self { operations }
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
        // find create message

        let collect_create_operation: Vec<OperationWithMeta> =
            self.operations_iter().filter(|op| op.is_create()).collect();

        if collect_create_operation.len() > 1 {
            return Err(DocumentBuilderError::MoreThanOneCreateOperation);
        } else if collect_create_operation.is_empty() {
            return Err(DocumentBuilderError::NoCreateOperation);
        }

        let create_operation = collect_create_operation.get(0).unwrap(); // unwrap as we know there is one item

        // Get the author of this document from the create message
        let author = create_operation.public_key();

        // Get the document id from the create message
        let document_id = create_operation.operation_id();

        // Get the document id from the create message
        let schema_hash = create_operation.schema();

        // Normally we would get the schema string from the DB by it's hash
        let schema = Schema::new(&schema_hash, DOCUMENT_SCHEMA)?;

        // Instantiate graph and operations map
        let mut graph = IncrementalTopo::new();
        let mut operations = BTreeMap::new();

        for op in self.operations() {
            // Validate each operation against the document schema before continuing.
            // NB. cddl crate not wasm supported yet.

            // schema.validate_operation_fields(&op.fields().unwrap())?;

            // Insert operation into map
            operations.insert(op.operation_id().as_str().to_owned(), op.to_owned());
            // Add node to graph
            graph.add_node(op);
        }

        // Derive graph dependencies from all operations' previous_operations field. Apply to graph handling
        // errors.
        // nb. I had some problems capturing the actual errors from IncrementalTopo crate... needs another
        // go at some point.
        operations.iter().try_for_each(|(_, successor)| {
            if let Some(previous_operations) = successor.previous_operations() {
                previous_operations.iter().try_for_each(|previous| {
                    match graph.add_dependency(
                        operations.get(previous.as_str()).unwrap(),
                        &successor.to_owned(),
                    ) {
                        Ok(_) => Ok(()),
                        Err(_) => Err(DocumentBuilderError::IncrementalTopoDependencyError),
                    }
                })
            } else {
                Ok(())
            }
        })?;

        Ok(Document {
            id: document_id.to_owned(),
            schema,
            author: author.to_owned(),
            operations,
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

    use crate::document::DocumentError;
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::{OperationValue, OperationWithMeta};
    use crate::test_utils::fixtures::{
        create_operation, fields, random_key_pair, schema, update_operation,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};
    use crate::Validate;

    use super::DocumentBuilder;

    #[rstest]
    fn sort_and_resolve_graph(
        schema: Hash,
        #[from(random_key_pair)] key_pair_1: KeyPair,
        #[from(random_key_pair)] key_pair_2: KeyPair,
    ) -> Result<(), DocumentError> {
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

        let document = DocumentBuilder::new(operations.clone()).build()?;

        // Document should be valid
        assert!(document.validate().is_ok());

        let instance = document.resolve()?;

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

        let replica_1 = DocumentBuilder::new(vec![
            op_5.clone(),
            op_4.clone(),
            op_3.clone(),
            op_2.clone(),
            op_1.clone(),
        ])
        .build()?;

        let replica_2 = DocumentBuilder::new(vec![
            op_3.clone(),
            op_2.clone(),
            op_1.clone(),
            op_5.clone(),
            op_4.clone(),
        ])
        .build()?;

        let replica_3 = DocumentBuilder::new(vec![
            op_2.clone(),
            op_1.clone(),
            op_4.clone(),
            op_3.clone(),
            op_5.clone(),
        ])
        .build()?;

        assert_eq!(replica_1.resolve().unwrap(), replica_2.resolve().unwrap());
        assert_eq!(replica_1.resolve().unwrap(), replica_3.resolve().unwrap());
        Ok(())
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
        let panda_entry_2_hash = send_to_node(
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

        let operations = node
            .all_entries()
            .iter()
            .filter(|entry| entry.hash() == panda_entry_2_hash)
            .map(|entry| {
                OperationWithMeta::new(&entry.entry_encoded(), &entry.operation_encoded()).unwrap()
            })
            .collect();

        assert!(DocumentBuilder::new(operations).build().is_err());
    }
}
