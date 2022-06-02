use log::debug;
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use super::GraphError;

/// This struct contains all functionality implemented in this module. It is can be used for
/// building and sorting a graph of causally connected nodes.
///
/// Sorting is deterministic with > comparison of contained node data being the deciding factor on
/// which paths to walk first.
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
/// graph.add_node(&'a', 'A');
/// graph.add_node(&'b', 'B');
/// graph.add_node(&'c', 'C');
///
/// assert!(graph.get_node(&'a').is_some());
/// assert!(graph.get_node(&'x').is_none());
///
/// // Add some links between the nodes.
///
/// graph.add_link(&'a', &'b');
/// graph.add_link(&'a', &'c');
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
/// assert_eq!(nodes.sorted(), vec!['A', 'B', 'C']);
///
/// // Add another link which creates a cycle (oh dear!).
///
/// graph.add_link(&'b', &'a');
///
/// assert!(graph.sort().is_err());
///
/// # Ok(())
/// # }
/// ```
#[derive(Debug, PartialEq, Clone)]
pub struct Graph<K, V>(HashMap<K, Node<K, V>>)
where
    K: Hash + Ord + PartialOrd + Eq + PartialEq + Clone + Debug,
    V: PartialEq + Clone + Debug;

/// An internal struct which represents a node in the graph and contains generic data.
#[derive(Debug, PartialEq, Clone)]
pub struct Node<K, V>
where
    K: Hash + Ord + PartialOrd + Eq + PartialEq + Clone + Debug,
    V: PartialEq + Clone + Debug,
{
    key: K,
    data: V,
    previous: Vec<K>,
    next: Vec<K>,
}

#[derive(Debug, PartialEq, Clone, Default)]
pub struct GraphData<V: PartialEq + Clone + Debug> {
    sorted: Vec<V>,
    graph_tips: Vec<V>,
}

impl<V: PartialEq + Clone + Debug> GraphData<V> {
    /// Returns the data from sorted graph nodes.
    pub fn sorted(&self) -> Vec<V> {
        self.sorted.clone()
    }

    /// Returns the current tips of this graph.
    pub fn current_graph_tips(&self) -> Vec<V> {
        self.graph_tips.clone()
    }
}

impl<'a, K, V> Node<K, V>
where
    K: Hash + Ord + PartialOrd + Eq + PartialEq + Clone + Debug,
    V: PartialEq + Clone + Debug,
{
    /// Returns true if this node is the root of this graph.
    fn is_root(&self) -> bool {
        self.previous.is_empty()
    }

    /// Returns true if this is a merge node.
    fn is_merge(&self) -> bool {
        self.previous.len() > 1
    }

    /// Returns true if this is a graph tip.
    fn is_tip(&self) -> bool {
        self.next.is_empty()
    }

    /// Returns the key for this node.
    fn key(&self) -> &K {
        &self.key
    }

    /// Returns a vector of keys for the nodes preceding this node in the graph.
    fn previous(&self) -> &Vec<K> {
        &self.previous
    }

    /// Returns a vector of keys for the nodes following this node in the graph.
    fn next(&self) -> &Vec<K> {
        &self.next
    }

    fn data(&self) -> V {
        self.data.clone()
    }
}

impl<'a, K, V> Graph<K, V>
where
    K: Hash + Ord + PartialOrd + Eq + PartialEq + Clone + Debug,
    V: PartialEq + Clone + Debug,
{
    /// Instantiate a new empty graph.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Add a node to the graph. This node will be detached until it is linked to another node.
    pub fn add_node(&mut self, key: &K, data: V) {
        let new_node = Node {
            key: key.clone(),
            next: Vec::new(),
            previous: Vec::new(),
            data,
        };

        self.0.insert(key.clone(), new_node);
    }

    /// Add a link between existing nodes to the graph. Returns true if the link was added.
    /// Returns false if the link was unable to be added. This happens if either of the nodes were
    /// not present in the graph, or if the link creates a single node loop.
    pub fn add_link(&mut self, from: &K, to: &K) -> bool {
        // Check for self-referential links
        if from == to {
            return false;
        }

        // Check that both nodes exist
        if self.get_node(from).is_none() || self.get_node(to).is_none() {
            return false;
        }

        // Add the outgoing link on the source
        self.0.get_mut(from).unwrap().next.push(to.to_owned());

        // Add the incoming link on the target
        self.0.get_mut(to).unwrap().previous.push(from.to_owned());

        true
    }

    /// Get node from the graph by key, returns `None` if it wasn't found.
    pub fn get_node(&'a self, key: &K) -> Option<&Node<K, V>> {
        self.0.get(key)
    }

    /// Returns a reference to the root node of this graph.
    pub fn root_node(&self) -> Result<&Node<K, V>, GraphError> {
        let root: Vec<&Node<K, V>> = self.0.values().filter(|node| node.is_root()).collect();
        match root.len() {
            0 => Err(GraphError::NoRootNode),
            1 => Ok(root[0]),
            _ => Err(GraphError::MultipleRootNodes),
        }
    }

    /// Returns the root node key.
    pub fn root_node_key(&self) -> Result<&K, GraphError> {
        match self.root_node() {
            Ok(root) => Ok(root.key()),
            Err(e) => Err(e),
        }
    }

    /// Check if all a nodes dependencies have been visited.
    fn dependencies_visited(&self, sorted: &[&Node<K, V>], node: &Node<K, V>) -> bool {
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
    fn next(&'a self, sorted: &[&Node<K, V>], node: &Node<K, V>) -> Option<Vec<&'a Node<K, V>>> {
        let mut next_nodes: Vec<&'a Node<K, V>> = Vec::new();

        for node_key in node.next() {
            // Nodes returned by `next()` have always been added by `add_link()`, which ensures
            // that these keys all have corresponding nodes in the graph so we can unwrap here.
            let node = self.get_node(node_key).unwrap();
            if !sorted
                .iter()
                .any(|sorted_node| sorted_node.key() == node.key())
            {
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

    /// Returns a new graph instance which has had all nodes removed which aren't causally linked
    /// to the passed nodes.
    ///
    /// Data contained in the returned nodes is unchanged, but the nodes' `next` arrays are edited
    /// to reflect the new graph connections. The returned graph can be sorted in order to arrive
    /// at a linear ordering of nodes.  
    pub fn trim(&'a mut self, tip_nodes: &[K]) -> Result<Graph<K, V>, GraphError> {
        // Instantiate the passed graph tips and make sure they exist in the graph.
        let mut graph_tips = vec![];
        for key in tip_nodes {
            let node = match self.get_node(key) {
                Some(node) => node.to_owned(),
                None => return Err(GraphError::InvalidTrimNodes),
            };
            graph_tips.push(node)
        }

        // The queue initially contains all the passed graph tips.
        let mut queue: Vec<&Node<K, V>> = graph_tips.iter().collect();
        let mut next_graph_nodes: HashMap<K, Node<K, V>> = HashMap::new();

        while let Some(mut current_node) = queue.pop() {
            if next_graph_nodes.len() > self.0.len() {
                return Err(GraphError::CycleDetected);
            }

            // Push the current node to the graph nodes array.
            next_graph_nodes.insert(current_node.key().to_owned(), current_node.clone());

            // We now start traversing the graph backwards from this point until we reach the root
            // or a merge node.
            while !current_node.previous().is_empty() {
                // Collect parent nodes from the graph by their key.
                let mut parent_nodes: Vec<&Node<K, V>> = current_node
                    .previous()
                    .iter()
                    // Unwrap as all nodes parents should exist in the graph.
                    .map(|key| self.get_node(key).unwrap())
                    .collect();

                for parent_node in parent_nodes.clone() {
                    // If a parent node has already been visited, we retrieve it, if not we set it's
                    // `next` field to an empty vec. This is done because we don't want to include any
                    // links to nodes which will be removed.
                    let mut parent_node_mut = match next_graph_nodes.get(parent_node.key()) {
                        Some(node) => node.to_owned(),
                        None => {
                            // If it hasn't we set it's `next` field to an empty
                            let mut parent_node = parent_node.clone();
                            parent_node.next = vec![];
                            parent_node
                        }
                    };

                    // Push the current node's key to the `next` field of the parent_node unless
                    // it is already included.
                    if !parent_node_mut.next.contains(current_node.key()) {
                        parent_node_mut.next.push(current_node.key().to_owned());
                    }

                    // Insert the parent_node into the next_graph_nodes.
                    next_graph_nodes.insert(parent_node.key().to_owned(), parent_node_mut.clone());
                }

                // If this is a merge node push all parent nodes back into the queue
                // and break out of the loop.
                if current_node.is_merge() {
                    for parent_node in parent_nodes {
                        queue.push(parent_node);
                    }
                    break;
                }

                // Pop and unwrap as we know there is exactly one node in the parent nodes.
                current_node = parent_nodes.pop().unwrap();
            }
        }

        // We need to edit the `next` field of every passed tip node so that they only contain nodes
        // which exist in the graph.
        let next_graph_nodes = next_graph_nodes
            .clone()
            .iter_mut()
            .map(|(key, node)| {
                let tips = node
                    .next
                    .iter()
                    .filter_map(|next_node| {
                        next_graph_nodes
                            .get(next_node)
                            .map(|node| node.key().to_owned())
                    })
                    .collect();

                node.next = tips;

                (key.to_owned(), node.to_owned())
            })
            .collect();

        Ok(Graph(next_graph_nodes))
    }

    /// Sorts the graph topologically and returns the result.
    pub fn walk_from(&'a self, key: &K) -> Result<GraphData<V>, GraphError> {
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
                "{:?}: sorted to position {}",
                current_node.key(),
                sorted_nodes.len()
            );

            // ...and then walk the graph starting from this node.
            while let Some(mut next_nodes) = self.next(&sorted_nodes, current_node) {
                // Pop off the next node we will visit.
                //
                // Nodes returned by `next()` have always been added by `add_link()`, which ensures
                // that these keys all have corresponding nodes in the graph so we can unwrap.
                let next_node = next_nodes.pop().unwrap();
                debug!("visiting: {:?}", next_node.key());

                // Push all other nodes connected to this one to the queue, we will visit these later.
                while let Some(node_to_be_queued) = next_nodes.pop() {
                    queue.push(node_to_be_queued);
                    debug!("{:?}: pushed to queue", node_to_be_queued.key());
                }

                // If it's a merge node, check it's dependencies have all been visited.
                if next_node.is_merge() {
                    if self.dependencies_visited(&sorted_nodes, next_node) {
                        // If they have been, push this node to the queue and exit this loop.
                        debug!(
                            "{:?}: is merge and has all dependencies met",
                            next_node.key()
                        );
                        queue.push(next_node);

                        debug!("{:?}: pushed to queue", next_node.key(),);

                        break;
                    } else if queue.is_empty() {
                        // The queue is empty, but this node has dependencies missing then there
                        // is either a cycle or missing nodes.
                        return Err(GraphError::BadlyFormedGraph);
                    }

                    debug!(
                        "{:?}: is merge and does not have dependencies met",
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
                    "{:?}: sorted to position {}",
                    next_node.key(),
                    sorted_nodes.len()
                );
                current_node = next_node;
            }
        }
        Ok(graph_data)
    }

    /// Sort the entire graph, starting from the root node.
    pub fn sort(&'a self) -> Result<GraphData<V>, GraphError> {
        let root_node = self.root_node_key()?;
        self.walk_from(root_node)
    }
}

impl<'a, K, V> Default for Graph<K, V>
where
    K: Hash + Ord + PartialOrd + Eq + PartialEq + Clone + Debug,
    V: PartialEq + Clone + Debug,
{
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
        let mut graph: Graph<char, char> = Graph::default();
        graph.add_node(&'a', 'A');
        graph.add_node(&'b', 'B');
        graph.add_node(&'c', 'C');
        graph.add_node(&'d', 'D');
        graph.add_node(&'e', 'E');
        graph.add_node(&'f', 'F');
        graph.add_node(&'g', 'G');
        graph.add_node(&'h', 'H');
        graph.add_node(&'i', 'I');
        graph.add_node(&'j', 'J');
        graph.add_node(&'k', 'K');

        // NB: unlinked nodes are simply not visited and do not exist in the sorted result.

        graph.add_link(&'a', &'b');
        graph.add_link(&'b', &'c');
        graph.add_link(&'c', &'d');
        graph.add_link(&'d', &'e');
        graph.add_link(&'e', &'f');

        // [A]<--[B]<--[C]<--[D]<--[E]<--[F]

        let expected = GraphData {
            sorted: vec!['A', 'B', 'C', 'D', 'E', 'F'],
            graph_tips: vec!['F'],
        };

        let graph_data = graph.walk_from(&'a').unwrap();

        assert_eq!(graph_data.sorted(), expected.sorted());
        assert_eq!(
            graph_data.current_graph_tips(),
            expected.current_graph_tips()
        );

        graph.add_link(&'a', &'g');
        graph.add_link(&'g', &'h');
        graph.add_link(&'h', &'d');

        //  /--[B]<--[C]--\
        // [A]<--[G]<-----[H]<--[D]<--[E]<---[F]

        let expected = GraphData {
            sorted: vec!['A', 'B', 'C', 'G', 'H', 'D', 'E', 'F'],
            graph_tips: vec!['F'],
        };

        let graph_data = graph.walk_from(&'a').unwrap();

        assert_eq!(graph_data.sorted(), expected.sorted());
        assert_eq!(
            graph_data.current_graph_tips(),
            expected.current_graph_tips()
        );

        graph.add_link(&'c', &'i');
        graph.add_link(&'i', &'j');
        graph.add_link(&'j', &'k');
        graph.add_link(&'k', &'f');

        //             /--[I]<--[J]<--[K]<--\
        //  /--[B]<--[C]--\                  \
        // [A]<--[G]<-----[H]<--[D]<--[E]<---[F]
        //

        let expected = GraphData {
            sorted: vec!['A', 'B', 'C', 'I', 'J', 'K', 'G', 'H', 'D', 'E', 'F'],
            graph_tips: vec!['F'],
        };

        let graph_data = graph.walk_from(&'a').unwrap();

        assert_eq!(graph_data.sorted(), expected.sorted());
        assert_eq!(
            graph_data.current_graph_tips(),
            expected.current_graph_tips()
        );
    }

    #[test]
    fn has_cycle() {
        let mut graph = Graph::new();

        graph.add_node(&'a', 1);
        graph.add_node(&'b', 2);
        graph.add_node(&'c', 3);
        graph.add_node(&'d', 4);

        // Can't add self-referential links
        assert!(!graph.add_link(&'a', &'a'));

        // Can't add links to non-existing nodes
        assert!(!graph.add_link(&'a', &'x'));

        graph.add_link(&'a', &'b');
        graph.add_link(&'b', &'c');
        graph.add_link(&'c', &'d');
        graph.add_link(&'d', &'b');

        assert!(graph.walk_from(&'a').is_err())
    }

    #[test]
    fn missing_dependencies() {
        let mut graph = Graph::new();
        graph.add_node(&'a', 1);
        graph.add_node(&'b', 2);
        graph.add_node(&'c', 3);
        graph.add_node(&'d', 4);

        graph.add_link(&'a', &'b');
        graph.add_link(&'b', &'c');
        graph.add_link(&'c', &'d');
        graph.add_link(&'d', &'b');
        graph.add_link(&'e', &'b'); // 'e' doesn't exist in the graph.

        assert!(graph.walk_from(&'a').is_err())
    }

    #[test]
    fn can_trim_graph() {
        let mut graph = Graph::new();
        graph.add_node(&'a', 1);
        graph.add_node(&'b', 2);
        graph.add_node(&'c', 3);
        graph.add_node(&'d', 4);
        graph.add_node(&'e', 5);

        graph.add_link(&'a', &'b');
        graph.add_link(&'b', &'c');
        graph.add_link(&'c', &'d');
        graph.add_link(&'c', &'e');

        let result = graph.trim(&['d', 'e']).unwrap().sort();
        assert_eq!(result.unwrap().sorted(), [1, 2, 3, 4, 5]);

        let result = graph.trim(&['c']).unwrap().sort();
        assert_eq!(result.unwrap().sorted(), [1, 2, 3]);

        let result = graph.trim(&['e']).unwrap().sort();
        assert_eq!(result.unwrap().sorted(), [1, 2, 3, 5]);

        let result = graph.trim(&['d']).unwrap().sort();
        assert_eq!(result.unwrap().sorted(), [1, 2, 3, 4]);
    }

    #[test]
    fn can_trim_more_complex_graph() {
        let mut graph = Graph::new();
        graph.add_node(&'a', 'A');
        graph.add_node(&'b', 'B');
        graph.add_node(&'c', 'C');
        graph.add_node(&'d', 'D');
        graph.add_node(&'e', 'E');
        graph.add_node(&'f', 'F');
        graph.add_node(&'g', 'G');
        graph.add_node(&'h', 'H');
        graph.add_node(&'i', 'I');
        graph.add_node(&'j', 'J');
        graph.add_node(&'k', 'K');

        graph.add_link(&'a', &'b');
        graph.add_link(&'b', &'c');
        graph.add_link(&'c', &'d');
        graph.add_link(&'d', &'e');
        graph.add_link(&'e', &'f');

        graph.add_link(&'a', &'g');
        graph.add_link(&'g', &'h');
        graph.add_link(&'h', &'d');

        graph.add_link(&'c', &'i');
        graph.add_link(&'i', &'j');
        graph.add_link(&'j', &'k');
        graph.add_link(&'k', &'f');

        //             /--[I]<--[J]<--[K]<--\
        //  /--[B]<--[C]--\                  \
        // [A]<--[G]<-----[H]<--[D]<--[E]<---[F]
        //

        let result = graph.trim(&['k']).unwrap().sort();
        assert_eq!(result.unwrap().sorted(), ['A', 'B', 'C', 'I', 'J', 'K']);

        let result = graph.trim(&['k', 'e']).unwrap().sort();
        assert_eq!(
            result.unwrap().sorted(),
            ['A', 'B', 'C', 'I', 'J', 'K', 'G', 'H', 'D', 'E']
        );

        let result = graph.trim(&['f']).unwrap().sort();
        assert_eq!(
            result.unwrap().sorted(),
            ['A', 'B', 'C', 'I', 'J', 'K', 'G', 'H', 'D', 'E', 'F']
        );

        let result = graph.trim(&['k', 'g']).unwrap().sort();
        assert_eq!(
            result.unwrap().sorted(),
            ['A', 'B', 'C', 'I', 'J', 'K', 'G']
        );

        // This is a weird case, many "tips" are passed, but they all exist in the same branch. It is valid though
        // and we process it correctly.
        let result = graph.trim(&['a', 'b', 'c', 'j']).unwrap().sort();
        assert_eq!(result.unwrap().sorted(), ['A', 'B', 'C', 'I', 'J']);
    }
    #[test]
    fn invalid_to_array() {
        let mut graph = Graph::new();
        graph.add_node(&'a', 1);
        graph.add_node(&'b', 2);
        graph.add_node(&'c', 3);
        graph.add_node(&'d', 4);
        graph.add_node(&'e', 5);

        graph.add_link(&'a', &'b');
        graph.add_link(&'b', &'c');
        graph.add_link(&'c', &'d');
        graph.add_link(&'c', &'e');

        let expected_err = "Requested trim nodes not found in graph";
        assert_eq!(
            graph.trim(&['d', 'f']).unwrap_err().to_string(),
            expected_err
        );
        assert_eq!(graph.trim(&['g']).unwrap_err().to_string(), expected_err);
    }

    #[test]
    fn poetic_graph() {
        let mut graph = Graph::new();
        graph.add_node(&'a', "Wake Up".to_string());
        graph.add_node(&'b', "Make Coffee".to_string());
        graph.add_node(&'c', "Drink Coffee".to_string());
        graph.add_node(&'d', "Stroke Cat".to_string());
        graph.add_node(&'e', "Look Out The Window".to_string());
        graph.add_node(&'f', "Start The Day".to_string());
        graph.add_node(&'g', "Cat Jumps Off Bed".to_string());
        graph.add_node(&'h', "Cat Meows".to_string());
        graph.add_node(&'i', "Brain Receives Caffeine".to_string());
        graph.add_node(&'j', "Brain Starts Engine".to_string());
        graph.add_node(&'k', "Brain Starts Thinking".to_string());

        graph.add_link(&'a', &'b');
        graph.add_link(&'b', &'c');
        graph.add_link(&'c', &'d');
        graph.add_link(&'d', &'e');
        graph.add_link(&'e', &'f');

        graph.add_link(&'a', &'g');
        graph.add_link(&'g', &'h');
        graph.add_link(&'h', &'d');

        graph.add_link(&'c', &'i');
        graph.add_link(&'i', &'j');
        graph.add_link(&'j', &'k');
        graph.add_link(&'k', &'f');

        assert_eq!(
            graph.walk_from(&'a').unwrap().sorted(),
            [
                "Wake Up".to_string(),
                "Make Coffee".to_string(),
                "Drink Coffee".to_string(),
                "Brain Receives Caffeine".to_string(),
                "Brain Starts Engine".to_string(),
                "Brain Starts Thinking".to_string(),
                "Cat Jumps Off Bed".to_string(),
                "Cat Meows".to_string(),
                "Stroke Cat".to_string(),
                "Look Out The Window".to_string(),
                "Start The Day".to_string()
            ]
        )
    }
}
