use log::debug;
use std::collections::HashMap;
use std::fmt::Debug;

use super::GraphError;

/// This struct contains all functionality implemented in this module. It is can be used for building and sorting a graph of
/// causally connected nodes.
///
/// Sorting is deterministic with > comparison of contained node data being the deciding factor on which paths to walk first.
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::graph::Graph;
///
/// // Instantiate the graph.
///
/// let mut graph = Graph::new();
///
/// // Add some nodes to the graph.
///
/// graph.add_node("a", "A");
/// graph.add_node("b", "B");
/// graph.add_node("c", "C");
///
/// assert!(graph.get_node("a").is_some());
/// assert!(graph.get_node("x").is_none());
///
/// // Add some links between the nodes.
///
/// graph.add_link("a", "b");
/// graph.add_link("a", "c");
///
/// // The graph looks like this:
/// //
/// //  /--[B]
/// // [A]<--[C]
///
/// // We can sort it topologically.
///
/// let nodes = graph.sort()?;
///
/// assert_eq!(nodes.sorted(), vec!["A", "B", "C"]);
///
/// // Add another link which creates a cycle (oh dear!).
///
/// graph.add_link("b", "a");
///
/// assert!(graph.sort().is_err());
///
/// # Ok(())
/// # }
/// ```
#[derive(Debug, PartialEq, Clone)]
pub struct Graph<T: PartialEq + Clone + Debug>(HashMap<String, Node<T>>);

/// An internal struct which represents a node in the graph and contains generic data.
#[derive(Debug, PartialEq, Clone)]
pub struct Node<T: PartialEq + Clone + Debug> {
    key: String,
    data: T,
    previous: Vec<String>,
    next: Vec<String>,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct GraphData<T: PartialEq + Clone + Debug> {
    sorted: Vec<T>,
    graph_tips: Vec<T>,
}

impl<T: PartialEq + Clone + Debug> GraphData<T> {
    /// Returns the data from sorted graph nodes.
    pub fn sorted(&self) -> Vec<T> {
        self.sorted.clone()
    }

    /// Returns the current tips of this graph.
    pub fn current_graph_tips(&self) -> Vec<T> {
        self.graph_tips.clone()
    }
}

impl<'a, T: PartialEq + Clone + Debug> Node<T> {
    /// Returns true if this node is the root of this graph.
    fn is_root(&self) -> bool {
        self.previous.is_empty()
    }

    /// Returns true if this is a merge node.
    fn is_merge(&self) -> bool {
        self.previous.len() > 1
    }

    /// Returns true if this is a branch node.
    fn is_branch(&self) -> bool {
        self.next.len() > 1
    }

    /// Returns true if this is a graph tip.
    fn is_tip(&self) -> bool {
        self.next.is_empty()
    }

    /// Returns the key for this node.
    fn key(&self) -> &String {
        &self.key
    }

    /// Returns a vector of keys for the nodes preceding this node in the graph.
    fn previous(&self) -> &Vec<String> {
        &self.previous
    }

    /// Returns a vector of keys for the nodes following this node in the graph.
    fn next(&self) -> &Vec<String> {
        &self.next
    }

    fn data(&self) -> T {
        self.data.clone()
    }
}

impl<'a, T: PartialEq + Clone + Debug> Graph<T> {
    /// Instantiate a new empty graph.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Add a node to the graph. This node will be detached until it is linked to another node.
    pub fn add_node(&mut self, key: &str, data: T) {
        let new_node = Node {
            key: key.to_string(),
            next: Vec::new(),
            previous: Vec::new(),
            data,
        };

        self.0.insert(key.to_string(), new_node);
    }

    /// Add a link between existing nodes to the graph. Returns true if the link was added.
    /// Returns false if the link was unable to be added. This happens if either of the nodes were not
    /// present in the graph, or if the link creates a single node loop.
    pub fn add_link(&mut self, from: &str, to: &str) -> bool {
        if from == to {
            return false;
        }

        if let Some(from_node_mut) = self.0.get_mut(from) {
            from_node_mut.next.push(to.to_owned());
        } else {
            return false;
        }

        if let Some(to_node_mut) = self.0.get_mut(to) {
            to_node_mut.previous.push(from.to_owned());
        } else {
            return false;
        }

        true
    }

    /// Get node from the graph by key, returns `None` if it wasn't found.
    pub fn get_node(&'a self, key: &str) -> Option<&Node<T>> {
        self.0.get(key)
    }

    /// Get the data payload from this node.
    pub fn get_node_data(&self, id: &str) -> Option<&T> {
        self.0.get(id).map(|node| &node.data)
    }

    /// Returns true if this node key is connected to the graph.
    pub fn is_connected(&self, key: &str) -> bool {
        if let Some(node) = self.get_node(key) {
            !node.previous().is_empty() || !node.next().is_empty()
        } else {
            false
        }
    }

    /// Returns the keys for nodes which follows this node key.
    pub fn get_next(&'a self, key: &str) -> Option<&Vec<String>> {
        self.get_node(key).map(|node| node.next())
    }

    /// Returns the keys for nodes which precede this node key.
    pub fn get_previous(&'a self, key: &str) -> Option<&Vec<String>> {
        self.get_node(key).map(|node| node.previous())
    }

    /// Returns true if this node key is a merge node.
    pub fn is_merge_node(&self, key: &str) -> bool {
        match self.get_node(key) {
            Some(node) => node.is_merge(),
            None => false,
        }
    }

    /// Returns true if this node key is a branch node.
    pub fn is_branch_node(&self, key: &str) -> bool {
        match self.get_node(key) {
            Some(node) => node.is_branch(),
            None => false,
        }
    }

    /// Returns true if this node key is a graph tip.
    pub fn is_tip_node(&self, key: &str) -> bool {
        match self.get_node(key) {
            Some(node) => node.is_tip(),
            None => false,
        }
    }

    /// Returns a reference to the root node of this graph.
    pub fn root_node(&self) -> Result<&Node<T>, GraphError> {
        let root: Vec<&Node<T>> = self.0.values().filter(|node| node.is_root()).collect();
        match root.len() {
            0 => Err(GraphError::NoRootNode),
            1 => Ok(root[0]),
            _ => Err(GraphError::MultipleRootNodes),
        }
    }

    /// Returns the root node key.
    pub fn root_node_key(&self) -> Result<&String, GraphError> {
        match self.root_node() {
            Ok(root) => Ok(root.key()),
            Err(e) => Err(e),
        }
    }

    /// Check if all a nodes dependencies have been visited.
    fn dependencies_visited(&self, sorted: &[&Node<T>], node: &Node<T>) -> bool {
        let mut has_dependencies = true;
        let previous_nodes = node.previous();

        for node_key in previous_nodes {
            let node = self.get_node(node_key).expect("Node not in graph");
            if !sorted.contains(&node) {
                has_dependencies = false
            }
        }

        has_dependencies
    }

    /// Returns the next un-visited node following the passed node.
    fn next(&'a self, sorted: &[&Node<T>], node: &Node<T>) -> Option<Vec<&'a Node<T>>> {
        let mut next_nodes: Vec<&'a Node<T>> = Vec::new();

        for node_key in node.next() {
            // Unwrap here as we are sure the node is in the graph.
            let node = self.get_node(node_key).expect("Node not in graph");
            if !sorted.contains(&node) {
                next_nodes.push(node)
            }
        }

        if next_nodes.is_empty() {
            return None;
        };
        next_nodes.sort_by_key(|node_a| node_a.key());
        next_nodes.reverse();
        Some(next_nodes)
    }

    /// Sorts the graph topologically and returns the sorted
    pub fn walk_from(&'a self, key: &str) -> Result<GraphData<T>, GraphError> {
        let root_node = match self.get_node(key) {
            Some(node) => Ok(node),
            None => Err(GraphError::NodeNotFound),
        }?;
        let mut queue = vec![root_node];
        let mut sorted_nodes = vec![];
        let mut graph_data = GraphData {
            sorted: vec![],
            graph_tips: vec![],
        };

        // Pop from the queue while it has items.
        while let Some(mut current_node) = queue.pop() {
            // If the sorted stack is bigger than the number of existing nodes we have a cycle.
            if sorted_nodes.len() > self.0.len() {
                return Err(GraphError::CycleDetected);
            }
            // Push this node to the sorted stack...
            sorted_nodes.push(current_node);
            graph_data.sorted.push(current_node.data());
            if current_node.is_tip() {
                graph_data.graph_tips.push(current_node.data())
            }
            debug!(
                "{}: sorted to position {}",
                current_node.key(),
                sorted_nodes.len()
            );

            // ...and then walk the graph starting from this node.
            while let Some(mut next_nodes) = self.next(&sorted_nodes, current_node) {
                // Pop off the next node we will visit.
                let next_node = next_nodes.pop().expect("Node not in graph");
                debug!("visiting: {}", next_node.key());

                // Push all other nodes connected to this one to the queue, we will visit these later.
                while let Some(node_to_be_queued) = next_nodes.pop() {
                    queue.push(node_to_be_queued);
                    debug!("{}: pushed to queue", node_to_be_queued.key());
                }

                // If it's a merge node, check it's dependencies have all been visited.
                if next_node.is_merge() {
                    if self.dependencies_visited(&sorted_nodes, next_node) {
                        // If they have been, push this node to the queue and exit this loop.
                        debug!("{}: is merge and has all dependencies met", next_node.key());
                        queue.push(next_node);

                        debug!("{}: pushed to queue", next_node.key(),);

                        break;
                    } else if queue.is_empty() {
                        // The queue is empty, but this node has dependencies missing then there
                        // is either a cycle or missing nodes.
                        return Err(GraphError::BadlyFormedGraph);
                    }

                    debug!(
                        "{}: is merge and does not have dependencies met",
                        next_node.key()
                    );
                    break;
                }
                // If it wasn't a merge node, push it to the sorted stack and keep walking.
                sorted_nodes.push(next_node);
                graph_data.sorted.push(next_node.data());

                // If it is a tip, push it to the graph tips list.
                if next_node.is_tip() {
                    graph_data.graph_tips.push(next_node.data());
                }

                debug!(
                    "{}: sorted to position {}",
                    next_node.key(),
                    sorted_nodes.len()
                );
                current_node = next_node;
            }
        }
        Ok(graph_data)
    }

    /// Sort the entire graph, starting from the root node.
    pub fn sort(&'a self) -> Result<GraphData<T>, GraphError> {
        let root_node = self.root_node_key()?;
        self.walk_from(root_node)
    }
}

impl<'a, T: PartialEq + Clone + Debug> Default for Graph<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use crate::graph::graph::GraphData;

    use super::Graph;

    #[test]
    fn basics() {
        let mut graph = Graph::new();
        graph.add_node("a", "A");
        graph.add_node("b", "B");
        graph.add_node("c", "C");
        graph.add_node("d", "D");
        graph.add_node("e", "E");
        graph.add_node("f", "F");
        graph.add_node("g", "G");
        graph.add_node("h", "H");
        graph.add_node("i", "I");
        graph.add_node("j", "J");
        graph.add_node("k", "K");

        // NB: unlinked nodes are simply not visited and do not exist in the sorted result.

        graph.add_link("a", "b");
        graph.add_link("b", "c");
        graph.add_link("c", "d");
        graph.add_link("d", "e");
        graph.add_link("e", "f");

        // [A]<--[B]<--[C]<--[D]<--[E]<--[F]

        let expected = GraphData {
            sorted: vec!["A", "B", "C", "D", "E", "F"],
            graph_tips: vec!["F"],
        };

        let graph_data = graph.walk_from("a").unwrap();

        assert_eq!(graph_data.sorted(), expected.sorted());
        assert_eq!(
            graph_data.current_graph_tips(),
            expected.current_graph_tips()
        );

        graph.add_link("a", "g");
        graph.add_link("g", "h");
        graph.add_link("h", "d");

        //  /--[B]<--[C]--\
        // [A]<--[G]<-----[H]<--[D]<--[E]<---[F]

        let expected = GraphData {
            sorted: vec!["A", "B", "C", "G", "H", "D", "E", "F"],
            graph_tips: vec!["F"],
        };

        let graph_data = graph.walk_from("a").unwrap();

        assert_eq!(graph_data.sorted(), expected.sorted());
        assert_eq!(
            graph_data.current_graph_tips(),
            expected.current_graph_tips()
        );

        graph.add_link("c", "i");
        graph.add_link("i", "j");
        graph.add_link("j", "k");
        graph.add_link("k", "f");

        //             /--[I]<--[J]<--[K]<--\
        //  /--[B]<--[C]--\                  \
        // [A]<--[G]<-----[H]<--[D]<--[E]<---[F]
        //

        let expected = GraphData {
            sorted: vec!["A", "B", "C", "I", "J", "K", "G", "H", "D", "E", "F"],
            graph_tips: vec!["F"],
        };

        let graph_data = graph.walk_from("a").unwrap();

        assert_eq!(graph_data.sorted(), expected.sorted());
        assert_eq!(
            graph_data.current_graph_tips(),
            expected.current_graph_tips()
        );
    }

    #[test]
    fn has_cycle() {
        let mut graph = Graph::new();
        graph.add_node("a", 1);
        graph.add_node("b", 2);
        graph.add_node("c", 3);
        graph.add_node("d", 4);

        graph.add_link("a", "b");
        graph.add_link("b", "c");
        graph.add_link("c", "d");
        graph.add_link("d", "b");

        assert!(graph.walk_from("a").is_err())
    }

    #[test]
    fn missing_dependencies() {
        let mut graph = Graph::new();
        graph.add_node("a", 1);
        graph.add_node("b", 2);
        graph.add_node("c", 3);
        graph.add_node("d", 4);

        graph.add_link("a", "b");
        graph.add_link("b", "c");
        graph.add_link("c", "d");
        graph.add_link("d", "b");
        graph.add_link("e", "b"); // "e" doesn't exist in the graph.

        assert!(graph.walk_from("a").is_err())
    }

    #[test]
    fn poetic_graph() {
        let mut graph = Graph::new();
        graph.add_node("a", "Wake Up");
        graph.add_node("b", "Make Coffee");
        graph.add_node("c", "Drink Coffee");
        graph.add_node("d", "Stroke Cat");
        graph.add_node("e", "Look Out The Window");
        graph.add_node("f", "Start The Day");
        graph.add_node("g", "Cat Jumps Off Bed");
        graph.add_node("h", "Cat Meows");
        graph.add_node("i", "Brain Receives Caffeine");
        graph.add_node("j", "Brain Starts Engine");
        graph.add_node("k", "Brain Starts Thinking");

        graph.add_link("a", "b");
        graph.add_link("b", "c");
        graph.add_link("c", "d");
        graph.add_link("d", "e");
        graph.add_link("e", "f");

        graph.add_link("a", "g");
        graph.add_link("g", "h");
        graph.add_link("h", "d");

        graph.add_link("c", "i");
        graph.add_link("i", "j");
        graph.add_link("j", "k");
        graph.add_link("k", "f");

        assert_eq!(
            graph.walk_from("a").unwrap().sorted(),
            [
                "Wake Up",
                "Make Coffee",
                "Drink Coffee",
                "Brain Receives Caffeine",
                "Brain Starts Engine",
                "Brain Starts Thinking",
                "Cat Jumps Off Bed",
                "Cat Meows",
                "Stroke Cat",
                "Look Out The Window",
                "Start The Day"
            ]
        )
    }
}
