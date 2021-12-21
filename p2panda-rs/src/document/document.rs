// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use crate::document::DocumentBuilderError;
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, OperationWithMeta};
use crate::schema::Schema;
use incremental_topo::IncrementalTopo;

/// Hard coded cddl string for now
const DOCUMENT_SCHEMA: &str = "wiki = { (
    title: { type: \"str\", value: tstr },
    content: { type: \"str\", value: tstr }
    wordcount: { type: \"int\", value: int }
) }";

/// A Document is a resolvable data type which is made up of a linked graph of operations. Documents MUST have a single root ‘CREATE’
/// operation. All other operations which mutate the initial data are inserted from this point and, due to the nature of operations,
/// connect together to form a directed acyclic graph.
///
/// The graph MUST contain only one root operation and there MUST be a path from the root to every other Operation contained in this
/// Document. All Operations MUST contain the hash id of both the Document it is operating on as well the previous known operation.
/// Documents MUST implement a method for topologically sorting the graph, iterating over the ordered list of operations, and applying
/// all updates onto an Instance following the document schema. This process MUST be deterministic, any Document replicas which
/// contain the same Operations MUST resolve to the same value.
///
/// All operations in a document MUST follow the documents Schema definition. This is defined by the root CREATE operation.
#[derive(Debug)]
pub struct Document {
    /// The hash id of this document, it is the hash of the entry of this documents root CREATE operation.
    id: Hash,
    /// The hash id of the schema operations in this document follow.
    schema: Schema,
    /// The author (public key) who published the CREATE message which instantiated this document.
    author: Author,
    /// Permissions, derived from KeyGroup relations which apply to the document as a whole or operation ranges within it.
    permissions: Option<Vec<Author>>,
    /// A map of all operations contained within this document. This may even include operations by unauthorized authors.
    operations: HashMap<String, OperationWithMeta>,
    /// A causal graph representation of this documents operations, identified by their hash, which can be topologically sorted.
    graph: IncrementalTopo<String>,
}

/// A struct for building documents.
#[derive(Debug)]
pub struct DocumentBuilder {
    /// An unsorted collection of operations which are associated with a particular document id.
    operations: Vec<OperationWithMeta>,
    /// Permissions for this document.
    /// TODO: don't know what form this will take yet, this is a placeholder for now.
    permissions: Option<Vec<Author>>,
}

impl DocumentBuilder {
    /// Instantiate a new DocumentBuilder with a collection of operations.
    pub fn new(mut operations: Vec<OperationWithMeta>) -> Self {
        operations.sort_by(|a, b| a.operation_id().as_str().cmp(b.operation_id().as_str()));
        Self {
            operations,
            permissions: None,
        }
    }

    /// Add permissions for this Document.
    pub fn permissions(mut self, permissions: Vec<Author>) -> Self {
        self.permissions = Some(permissions);
        self
    }

    /// Build the document.
    pub fn build(self) -> Result<Document, DocumentBuilderError> {
        // find create message

        let collect_create_operation: Vec<&OperationWithMeta> =
            self.operations.iter().filter(|op| op.is_create()).collect();

        if collect_create_operation.len() > 1 || collect_create_operation.is_empty() {
            // Error
        }

        let create_operation = collect_create_operation.get(0).unwrap(); // unwrap as we know there is one item

        // Get the author of this document from the create message
        let author = create_operation.public_key();

        // Get the document id from the create message
        let document_id = create_operation.operation_id();

        // Get the document id from the create message
        let schema_hash = create_operation.schema();

        // Normally we would get the schema string from the DB by it's hash
        let schema = Schema::new(&schema_hash, DOCUMENT_SCHEMA).unwrap();

        // Instantiate graph and operations map
        let mut graph = IncrementalTopo::new();
        let mut operations = HashMap::new();

        self.operations.iter().for_each(|op| {
            // Insert operation into map
            operations.insert(op.operation_id().as_str().to_owned(), op.to_owned());
            // Add node to graph
            graph.add_node(op.operation_id().as_str().to_string());
        });

        // Derive graph dependencies from all operations' previous_operations field. Apply to graph handling
        // errors.
        // nb. I had some problems capturing the actual errors from IncrementalTopo crate... needs another
        // go at some point.
        self.operations
            .iter()
            .try_for_each(|successor: &OperationWithMeta| {
                if let Some(operations) = successor.previous_operations() {
                    operations.iter().try_for_each(|previous| {
                        match graph.add_dependency(
                            &previous.as_str().to_owned(),
                            &successor.operation_id().as_str().to_owned(),
                        ) {
                            Ok(_) => Ok(()),
                            Err(_) => Err(DocumentBuilderError::IncrementalTopoDepenedencyError),
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
            permissions: None,
            operations,
            graph,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::DocumentBuilder;
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::{AsOperation, OperationValue, OperationWithMeta};
    use crate::test_utils::fixtures::{
        create_operation, fields, random_key_pair, schema, update_operation,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};
    use rstest::rstest;

    #[rstest]
    fn as_node(
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
                    "cafe_name",
                    OperationValue::Text("Panda Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Panda publishes an update operation.
        // It contains the hash of the previous operation in it's `previous_operations` array
        let panda_entry_2_hash = send_to_node(
            &mut node,
            &panda,
            &update_operation(
                schema.clone(),
                panda_entry_1_hash.clone(),
                vec![panda_entry_1_hash.clone()],
                fields(vec![(
                    "cafe_name",
                    OperationValue::Text("Panda Cafe!".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Penguin publishes an update operation which creates a new branch in the graph.
        // This is because they didn't know about Panda's second operation.
        let penguin_entry_1_hash = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                panda_entry_1_hash.clone(),
                vec![panda_entry_1_hash.clone()],
                fields(vec![(
                    "cafe_name",
                    OperationValue::Text("Penguin Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous_operations` field now contains 2 operation hash id's.
        let penguin_entry_2_hash = send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema.clone(),
                panda_entry_1_hash.clone(),
                vec![penguin_entry_1_hash.clone(), panda_entry_2_hash],
                fields(vec![(
                    "cafe_name",
                    OperationValue::Text("Polar Bear Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        // Penguin publishes a new operation while now being aware of the previous branching situation.
        // Their `previous_operations` field now contains 2 operation hash id's.
        send_to_node(
            &mut node,
            &penguin,
            &update_operation(
                schema,
                panda_entry_1_hash.clone(),
                vec![penguin_entry_1_hash, penguin_entry_2_hash],
                fields(vec![(
                    "cafe_name",
                    OperationValue::Text("Polar Bear Cafe!!!!!!!!!!".to_string()),
                )]),
            ),
        )
        .unwrap();

        let entries = node
            .all_entries()
            .iter()
            .map(|entry| {
                OperationWithMeta::new(&entry.entry_encoded(), &entry.operation_encoded()).unwrap()
            })
            .collect();

        let document = DocumentBuilder::new(entries).build().unwrap();

        let descendents = document
            .graph
            .descendants(panda_entry_1_hash.as_str())
            .unwrap();

        for hash in descendents {
            if let Some(op) = document.operations.get(hash) {
                println!("{:?}", op.fields().unwrap().get("cafe_name"))
            }
        }
    }
}
