/// Wrapper type for a Node in the graph
type Node = String;

/// Wrapper type for an Edge in the graph
type Edge = (Option<Node>, Node);

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
            /// Entry hashes [("00x42asd...", "00x435d..."), .... ], but it can be any string.
            /// The first string in the tuple is optional as the root of the graph has no parent.
            graph: Vec::new(),
        }
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

    /// Validate the DAG.
    ///
    /// @TODO:
    /// - Check there is exactly one root node
    /// - Check there are no disconnected nodes
    /// - Check there are no cycles
    ///
    /// QUESTION: is this necessary? All situations are impossible when processing entries in a Bamboo log.
    pub fn validate() -> () {
        // todo
    }
}
