// SPDX-License-Identifier: AGPL-3.0-or-later

/// A rust port of tangle-js graph module https://gitlab.com/tangle-js/tangle-graph
use crate::graph::node::GraphNode;
use crate::hash::Hash;

use super::error::GraphNodeError;

pub trait AsGraph {
    ///////////////////////
    // tangle-js methods //
    ///////////////////////

    /// Get a node from the graph by key. If the node doesn't exist or is disconnected, returns an error.
    fn get_node(&self, key: Hash) -> Result<GraphNode, GraphNodeError>;

    /// Check if a node is connected to the graph by key
    fn is_connected(&self, key: Hash) -> bool;

    /// Returns an Array of keys of nodes that are causally linked to this node-key.
    /// This contains keys of nodes after this key-node in the graph.
    fn get_next(&self, key: Hash) -> Vec<GraphNode>;

    /// Returns the previous property for a given node.
    /// This contains keys of nodes before this key-node in the graph.
    fn get_previous(&self, key: Hash) -> Vec<GraphNode>;

    /// Returns true if the graph diverges as you proceed from the given node-key.
    fn is_branch_node(&self, key: Hash) -> bool;

    /// returns true if the given node-key has previous.len() > 1
    fn is_merge_node(&self, key: Hash) -> bool;

    /// Returns true if the given key-node has no successor (ie. next) nodes. This means it is a leading tip
    /// of a branch on the graph.
    fn is_tip_node(&self, key: Hash) -> bool;

    // fn invalidate_keys(&self, Vec<Hash>);

    /// returns the root node of this graph.
    fn root_node(&self) -> GraphNode;

    /// returns the key of the root node of this graph.
    fn root_node_key(&self) -> Hash;

    // fn raw(&self);

    // fn get_history(&self, key: Hash) -> Vec<Hash>;

    /////////////////////
    // p2panda methods //
    /////////////////////

    //tbc...
}
