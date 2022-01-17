use std::collections::HashMap;

/// Directed acyclic casaul graph who's nodes can be sorted topologically.
///
/// Graph API based on [tangle-graph](https://gitlab.com/tangle-js/tangle-graph).
#[derive(Debug)]
pub struct Graph<T: PartialEq + Clone>(HashMap<String, Node<T>>);

#[derive(Debug, PartialEq, Clone)]
pub struct Node<T: PartialEq + Clone> {
    key: String,
    data: T,
    previous: Vec<String>,
    next: Vec<String>,
}

impl<T: PartialEq + Clone> Node<T> {
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
    fn key(&self) -> String {
        self.key.to_owned()
    }

    /// Returns a vector of keys for the nodes preceding this node in the graph.
    fn previous(&self) -> Vec<String> {
        self.previous.clone()
    }

    /// Returns a vector of keys for the nodes following this node in the graph.
    fn next(&self) -> Vec<String> {
        self.next.clone()
    }

    fn data(&self) -> T {
        self.data.clone()
    }
}

impl<'a, T: PartialEq + Clone> Graph<T> {
    /// Instantiate a new empty graph.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Instantiate a graph from a vec of nodes.
    pub fn new_from_nodes(nodes: Vec<Node<T>>) -> Self {
        let mut graph = HashMap::new();
        for node in &nodes {
            graph.insert(node.key(), node.to_owned());
        }

        let mut graph = Self(graph);

        for node in nodes {
            for previous in node.previous() {
                graph.add_link(&previous, &node.key())
            }
        }

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

    /// Add a link between existing nodes to the graph.
    pub fn add_link(&mut self, from: &str, to: &str) {
        if let Some(from_node) = self.get_node_mut_by_id(from) {
            from_node.next.push(to.to_string())
        }

        if let Some(to_node) = self.get_node_mut_by_id(to) {
            to_node.previous.push(from.to_string())
        }
    }

    /// Get node from the graph by key, returns `None` if it wasn't found.
    pub fn get_node(&self, key: &str) -> Option<&Node<T>> {
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
    pub fn get_next(&self, key: &str) -> Option<Vec<String>> {
        self.get_node(key).map(|node| node.next())
    }

    /// Returns the keys for nodes which precede this node key.
    pub fn get_previous(&self, key: &str) -> Option<Vec<String>> {
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

    // NOT IMPLEMENTED //
    // pub fn invalidate_keys(&self, keys: Vec<String>) {}

    /// Returns a reference to the root node of this graph.
    pub fn root_node(&self) -> &Node<T> {
        self.0.values().find(|node| node.is_root()).unwrap()
    }

    /// Returns the root node key.
    pub fn root_node_key(&self) -> String {
        self.0.values().find(|node| node.is_root()).unwrap().key()
    }

    /// Get a mutable reference to a node in the graph identified by it's key.
    fn get_node_mut_by_id(&mut self, id: &str) -> Option<&mut Node<T>> {
        self.0.get_mut(id)
    }

    /// Check if all a nodes dependencies have been visited.
    fn dependencies_visited(&self, sorted: &[String], node: &Node<T>) -> bool {
        let mut has_dependencies = true;
        for previous_node in node.previous() {
            if !sorted.contains(&previous_node) {
                has_dependencies = false
            }
        }
        has_dependencies
    }

    /// Returns the next un-visited node following the passed node.
    fn next(&self, sorted: &[String], node: &'a Node<T>) -> Option<Vec<String>> {
        let mut next_node_keys = Vec::new();
        for link in &node.next() {
            if !sorted.contains(link) {
                next_node_keys.push(link.to_string())
            };
        }
        if next_node_keys.is_empty() {
            return None;
        };
        next_node_keys.reverse();
        Some(next_node_keys)
    }

    /// Sorts the graph topologically and returns the sorted
    pub fn walk(&'a mut self) -> Vec<T> {
        let root_node = self.0.values().find(|node| node.is_root()).unwrap();
        let mut queue = vec![root_node];
        let mut sorted = Vec::new();

        // Helper closure for pushing node to the queue.
        let push_to_queue = |queue: &mut Vec<&'a Node<T>>, node_key: &str| {
            let node = self.get_node(node_key).unwrap();
            if !queue.contains(&node) {
                println!("{}: push to queue", node_key);
                queue.push(node)
            } else {
                println!("{}: already in queue", node_key);
            };
        };

        // Helper closure for pushing node to the sorted stack.
        let push_to_sorted = |sorted: &mut Vec<String>, node_key: String| {
            sorted.push(node_key.clone());
            println!("{}: sorted to postion {}", node_key, sorted.len());
        };

        // Pop from the queue while it has items.
        while let Some(mut current_node) = queue.pop() {
            // Push this node to the sorted stack...
            push_to_sorted(&mut sorted, current_node.key());

            // ...and then walk the graph starting from this node.
            while let Some(mut next_node_keys) = self.next(&sorted, current_node) {
                // Pop off the next node we will visit.
                let next_node_key = next_node_keys.pop().unwrap();

                // Push all other nodes connected to this one to the queue, we will visit these later.
                while let Some(node_to_be_queued) = next_node_keys.pop() {
                    push_to_queue(&mut queue, &node_to_be_queued)
                }

                // Retrieve the next node by it's key.
                if let Some(next_node) = self.get_node(&next_node_key) {
                    // If it's a merge node, check it's dependencies have all been visited.
                    if next_node.is_merge() {
                        if self.dependencies_visited(&sorted, next_node) {
                            // If they have been, push this node to the queue and exit this loop.
                            push_to_queue(&mut queue, &next_node.key());
                            println!("{}: is merge and has all dependencies met", next_node.key());
                            break;
                        }
                        // Else don't do anything and break out of this loop.
                        println!(
                            "{}: is merge and does not have dependencies met",
                            next_node.key()
                        );
                        break;
                    }
                    // If it wasn't a merge node, push it to the sorted stack and keep walking.
                    push_to_sorted(&mut sorted, next_node.key());
                    current_node = next_node;
                }
            }
        }
        sorted
            .iter()
            .map(|key| self.get_node(key).unwrap().data())
            .collect()
    }
}

#[cfg(test)]
mod test {
    use super::Graph;

    #[test]
    fn basics() {
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
            graph.walk(),
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
