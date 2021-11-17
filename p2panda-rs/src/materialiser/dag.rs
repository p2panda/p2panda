// SPDX-License-Identifier: AGPL-3.0-or-later

/// Wrapper type for a Node in the graph
pub type Node = String;

/// Wrapper type for an Edge in the graph
pub type Edge = (Option<Node>, Node);

/// A directed acyclic graph which can be ordered topologically in a depth-first sort. It is
/// described by a list of `edges` which in turn descirbe connections between parent and child
/// nodes.
#[derive(Clone, Debug)]
pub struct DAG {
    // the DAG structure
    graph: Vec<Edge>,
}

impl DAG {
    pub fn new() -> Self {
        DAG {
            /// An array of edges which make up the graph. For p2p2anda this is an array of tuples of
            /// Entry hashes [(Some("00x42asd..."), "00x435d..."), .... ], but it can be any string.
            /// The first string in the tuple is optional as the root of the graph has no parent.
            graph: Vec::new(),
        }
    }

    /// New DAG from an existing array of edges.
    pub fn new_from_graph(graph: Vec<Edge>) -> Self {
        DAG { graph: graph }
    }

    /// Return graph edges as array.
    pub fn graph(&self) -> Vec<Edge> {
        self.graph.to_owned()
    }

    /// Add a root node to the graph.
    pub fn add_root(&mut self, node_id: Node) {
        self.graph.insert(0, (None, node_id.into()));
    }

    /// Add an edge to the graph.
    pub fn add_edge(&mut self, from: Node, to: Node) {
        self.graph.insert(0, (Some(from.into()), to.into()));
    }

    /// Return all out edges starting from a given node.
    pub fn node_out_edges(&self, current_node: &Node) -> Option<Vec<Edge>> {
        // Collect all edges where this node is the parent.
        let mut out_edges: Vec<Edge> = self
            .graph()
            .iter()
            .filter(|(from, _to)| match from {
                Some(f) => f == current_node,
                None => false,
            })
            .cloned()
            .collect();

        // Sort edges in alphabetical order according to the hash id of the entry addressed by the out_edge.
        // This means our topological sorting will be consistent across nodes who know about the same entries.
        out_edges.sort_by(|(_, out_edge_a), (_, out_edge_b)| out_edge_a.cmp(out_edge_b));

        // If there are no edges then this is the end of this branch we should return None
        if out_edges.len() == 0 {
            None
        } else {
            Some(out_edges)
        }
    }

    /// Find the initial starting node for this DAG (the node with no parent)
    pub fn initial_root(&self) -> Option<Node> {
        let mut root = None;
        for (parent, child) in self.graph.iter() {
            match parent {
                Some(_) => continue,
                None => root = Some(child.to_owned()),
            }
        }
        root
    }

    /// Perform depth-first traversal of DAG, merging all forks, and returns an ordered list of
    /// nodes.
    pub fn topological(&mut self) -> Vec<Node> {
        // Array of queued graph nodes
        let mut queue: Vec<Node> = Vec::new();

        // Topologically ordered graph nodes
        let mut ordered_nodes: Vec<Node> = Vec::new();

        // The root node of this graph
        let root = self.initial_root();

        // Insert root node into queue if it exists
        match root {
            Some(node) => queue.insert(0, node),
            None => (),
        }

        // Pop next root node from end of queue.
        // Continue while there are items in the queue.
        while let Some(mut current_node) = queue.pop() {
            // Push the current node into final array of ordered nodes. This means it has been
            // visited, we don't need a visited nodes array as we are using a queue.
            ordered_nodes.push(current_node.to_owned());

            // Walk from this node until reaching a tip (leaf) of the graph (a node with no edges).
            // edges are returned in alphabetical order which is how we consistently resolve concurrent edits
            // (last write wins).
            while let Some(mut out_edges) = self.node_out_edges(&mut current_node) {
                // The next node we will visit
                let next_edge = out_edges.remove(0);

                // Any other target nodes are pushed to the queue for later walking
                for edge in out_edges {
                    queue.insert(0, edge.1.to_owned());
                }

                // Push the next node we are visiting to the ordered_nodes array
                ordered_nodes.push(next_edge.1.clone());

                // Set the new current_node
                current_node = next_edge.1;
            }
        }
        ordered_nodes
    }
}

#[cfg(test)]
mod tests {
    use super::DAG;

    use crate::entry::{sign_and_encode, Entry, LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::materialiser::{marshall_entries, Edge};
    use crate::message::{Message, MessageEncoded, MessageFields, MessageValue};

    use rstest::{fixture, rstest};
    use std::convert::TryFrom;

    // All this boilerplate to be removed once `p2panda-tests` is merged.
    fn create_message_fields(keys: Vec<&str>, values: Vec<&str>) -> MessageFields {
        let mut fields = MessageFields::new();
        for (pos, key) in keys.iter().enumerate() {
            fields
                .add(
                    key.to_owned(),
                    MessageValue::Text(values.get(pos).unwrap().to_string()),
                )
                .unwrap();
        }
        fields
    }

    #[fixture]
    fn key_pair() -> KeyPair {
        KeyPair::new()
    }

    #[fixture]
    fn message(
        #[default(vec!["message"])] keys: Vec<&str>,
        #[default(vec!["Hello!"])] values: Vec<&str>,
    ) -> Message {
        let fields = create_message_fields(keys, values);
        Message::new_create(Hash::new_from_bytes(vec![1, 2, 3]).unwrap(), fields).unwrap()
    }

    #[fixture]
    fn entry(
        message: Message,
        #[default(SeqNum::new(1).unwrap())] seq_num: SeqNum,
        #[default(None)] backlink: Option<Hash>,
        #[default(None)] skiplink: Option<Hash>,
    ) -> Entry {
        Entry::new(
            &LogId::default(),
            Some(&message),
            skiplink.as_ref(),
            backlink.as_ref(),
            &seq_num,
        )
        .unwrap()
    }

    #[test]
    fn topological_sort() {
        // Same graph construct in different orders should order in the same way

        let ordered_dag = vec![
            "0x0a", "0x1a", "0x2a", "0x3a", "0x4a", "0x2b", "0x3b", "0x3c", "0x4c",
        ];

        let mut graph_1 = DAG::new();

        // DAG trunk A
        graph_1.add_root("0x0a".to_string());
        graph_1.add_edge("0x0a".to_string(), "0x1a".to_string());
        graph_1.add_edge("0x1a".to_string(), "0x2a".to_string());
        graph_1.add_edge("0x2a".to_string(), "0x3a".to_string());
        graph_1.add_edge("0x3a".to_string(), "0x4a".to_string());

        // Fork B
        graph_1.add_edge("0x1a".to_string(), "0x2b".to_string());
        graph_1.add_edge("0x2b".to_string(), "0x3b".to_string());

        // Fork C
        graph_1.add_edge("0x2a".to_string(), "0x3c".to_string());
        graph_1.add_edge("0x3c".to_string(), "0x4c".to_string());

        assert_eq!(graph_1.topological(), ordered_dag);

        let mut graph_2 = DAG::new();

        // DAG trunk A
        graph_2.add_root("0x0a".to_string());
        graph_2.add_edge("0x0a".to_string(), "0x1a".to_string());
        graph_2.add_edge("0x1a".to_string(), "0x2a".to_string());
        graph_2.add_edge("0x2a".to_string(), "0x3a".to_string());
        graph_2.add_edge("0x3a".to_string(), "0x4a".to_string());

        // Fork C
        graph_2.add_edge("0x2a".to_string(), "0x3c".to_string());
        graph_2.add_edge("0x3c".to_string(), "0x4c".to_string());

        // Fork B
        graph_2.add_edge("0x1a".to_string(), "0x2b".to_string());
        graph_2.add_edge("0x2b".to_string(), "0x3b".to_string());

        assert_eq!(graph_2.topological(), ordered_dag);

        let mut graph_3 = DAG::new();

        // DAG trunk A
        graph_3.add_root("0x0a".to_string());
        graph_3.add_edge("0x0a".to_string(), "0x1a".to_string());
        graph_3.add_edge("0x1a".to_string(), "0x2a".to_string());

        // Fork C
        graph_3.add_edge("0x2a".to_string(), "0x3c".to_string());
        graph_3.add_edge("0x3c".to_string(), "0x4c".to_string());

        // Fork B
        graph_3.add_edge("0x1a".to_string(), "0x2b".to_string());
        graph_3.add_edge("0x2b".to_string(), "0x3b".to_string());

        // DAG trunk A
        graph_3.add_edge("0x2a".to_string(), "0x3a".to_string());
        graph_3.add_edge("0x3a".to_string(), "0x4a".to_string());

        assert_eq!(graph_3.topological(), ordered_dag)
    }

    #[rstest]
    fn marshall_entries_test(entry: Entry, key_pair: KeyPair) {
        let encoded_message = MessageEncoded::try_from(entry.message().unwrap()).unwrap();

        let signed_encoded_entry = sign_and_encode(&entry, &key_pair).unwrap();

        let message_fields_2 = create_message_fields(vec!["message"], vec!["Hello too!"]);
        let message_2 = Message::new_update(
            Hash::new_from_bytes(vec![1, 2, 3]).unwrap(),
            signed_encoded_entry.hash(),
            message_fields_2,
        )
        .unwrap();

        let encoded_message_2 = MessageEncoded::try_from(&message_2).unwrap();

        let entry = Entry::new(
            &LogId::default(),
            Some(&message_2),
            None,
            Some(&signed_encoded_entry.hash()),
            &SeqNum::new(2).unwrap(),
        )
        .unwrap();

        let signed_encoded_entry_2 = sign_and_encode(&entry, &key_pair).unwrap();

        let edges = marshall_entries(vec![
            (signed_encoded_entry.clone(), encoded_message),
            (signed_encoded_entry_2.clone(), encoded_message_2),
        ])
        .unwrap();

        let edge_1: Edge = (None, signed_encoded_entry.hash().as_str().to_owned());
        let edge_2: Edge = (
            Some(signed_encoded_entry.hash().as_str().to_owned()),
            signed_encoded_entry_2.hash().as_str().to_string(),
        );

        assert_eq!(edges, vec![edge_1, edge_2]);
    }
}
