// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::{AsOperation, OperationWithMeta};
use crate::schema::Schema;
use incremental_topo::IncrementalTopo;

// Instanciate "person" schema from cddl string
const DOCUMENT_SCHEMA: &str = "wiki = { (
    title: { type: \"str\", value: tstr },
    content: { type: \"str\", value: tstr }
    wordcount: { type: \"int\", value: int }
) }";

#[derive(Debug)]
pub struct Document {
    // Could use a BiMap here. Is that cool?
    id: Hash,
    schema: Schema,
    author: Author,
    permissions: Option<Vec<Author>>,
    operations: HashMap<String, OperationWithMeta>,
    graph: IncrementalTopo<String>,
}

pub struct DocumentBuilder {
    permissions: Option<Vec<Author>>,
    operations: Vec<OperationWithMeta>,
}

impl DocumentBuilder {
    pub fn new(mut operations: Vec<OperationWithMeta>) -> Self {
        operations.sort_by(|a, b| a.operation_id().as_str().cmp(b.operation_id().as_str()));
        Self {
            operations,
            permissions: None,
        }
    }

    pub fn permissions(mut self, permissions: Vec<Author>) -> Self {
        self.permissions = Some(permissions);
        self
    }

    pub fn build(self) -> Document {
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

        self.operations.iter().for_each(|op| {
            // add dependencies derived from each operations previous_operations
            if let Some(ops) = op.previous_operations() {
                ops.iter().for_each(|id| {
                    graph.add_dependency(
                        &id.as_str().to_string(),
                        &op.operation_id().as_str().to_string(),
                    );
                })
            };
        });

        Document {
            id: document_id.to_owned(),
            schema,
            author: author.to_owned(),
            permissions: None,
            operations,
            graph,
        }
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

        let document = DocumentBuilder::new(entries).build();

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
