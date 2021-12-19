// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::{Entry, EntrySigned};
use crate::graph::error::GraphNodeError;
use crate::hash::Hash;
use crate::operation::{AsOperation, OperationEncoded, OperationFields, OperationWithMeta};

pub struct GraphNode(OperationWithMeta);

impl GraphNode {
    pub fn new(
        entry_encoded: &EntrySigned,
        operation_encoded: &OperationEncoded,
    ) -> Result<Self, GraphNodeError> {
        let graph_node = Self(OperationWithMeta::new(entry_encoded, operation_encoded)?);
        Ok(graph_node)
    }
}

pub trait AsNode {
    fn key(&self) -> &Hash;

    fn previous(&self) -> Option<&Vec<Hash>>;

    fn data(&self) -> Option<&OperationFields>;

    fn is_root(&self) -> bool {
        self.previous().is_none()
    }

    fn has_many_previous(&self) -> bool {
        match self.previous() {
            Some(previous) => previous.len() > 1,
            None => false,
        }
    }
}

impl AsNode for GraphNode {
    fn key(&self) -> &Hash {
        self.0.operation_id()
    }

    fn previous(&self) -> Option<&Vec<Hash>> {
        self.0.previous_operations()
    }

    fn data(&self) -> Option<&OperationFields> {
        self.0.fields()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::{OperationValue, OperationWithMeta};
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, fields, random_key_pair, schema, update_operation,
    };
    use crate::test_utils::mocks::{send_to_node, Client, Node};

    use super::{AsNode, GraphNode};

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
                schema,
                panda_entry_1_hash.clone(),
                vec![penguin_entry_1_hash.clone(), panda_entry_2_hash.clone()],
                fields(vec![(
                    "cafe_name",
                    OperationValue::Text("Polar Bear Cafe".to_string()),
                )]),
            ),
        )
        .unwrap();

        let entries = node.all_entries();
        let entry_1 = entries.get(0).unwrap();
        let entry_2 = entries.get(1).unwrap();
        let entry_3 = entries.get(2).unwrap();
        let entry_4 = entries.get(3).unwrap();

        let graph_node_1 =
            GraphNode::new(&entry_1.entry_encoded(), &entry_1.operation_encoded()).unwrap();
        let graph_node_2 =
            GraphNode::new(&entry_2.entry_encoded(), &entry_2.operation_encoded()).unwrap();
        let graph_node_3 =
            GraphNode::new(&entry_3.entry_encoded(), &entry_3.operation_encoded()).unwrap();
        let graph_node_4 =
            GraphNode::new(&entry_4.entry_encoded(), &entry_4.operation_encoded()).unwrap();

        // Node 1 is the root node and has no previous operations
        assert_eq!(graph_node_1.key(), &panda_entry_1_hash);
        assert!(graph_node_1.is_root());
        assert!(!graph_node_1.has_many_previous());
        // Node 2 is not the root node and has one previous operations
        assert_eq!(graph_node_2.key(), &panda_entry_2_hash);
        assert!(!graph_node_2.is_root());
        assert!(!graph_node_2.has_many_previous());
        // Node 3 is not the root node and has one previous operations
        assert_eq!(graph_node_3.key(), &penguin_entry_1_hash);
        assert!(!graph_node_3.is_root());
        assert!(!graph_node_3.has_many_previous());
        // Node 4 is not the root node and has more than 1 previous operations
        assert_eq!(graph_node_4.key(), &penguin_entry_2_hash);
        assert!(!graph_node_4.is_root());
        assert!(graph_node_4.has_many_previous());
    }
}
