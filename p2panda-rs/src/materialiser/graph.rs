use std::collections::HashMap;

#[derive(Debug)]
pub struct Graph<T: PartialEq> {
    nodes: HashMap<String, Node<T>>,
    visited: Vec<String>,
    sorted: Vec<String>,
    queue: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct Node<T: PartialEq> {
    id: String,
    elem: T,
    previous: Vec<String>,
    next: Vec<String>,
}

pub enum NodeResult<'a, T: PartialEq> {
    Connected(&'a Node<T>),
    Disconnected(&'a Node<T>),
    NotFound,
}

impl<T: PartialEq> Node<T> {
    pub fn is_root(&self) -> bool {
        self.previous.is_empty()
    }

    pub fn is_merge(&self) -> bool {
        self.previous.len() > 1
    }

    pub fn is_branch(&self) -> bool {
        self.next.len() > 1
    }

    pub fn is_tip(&self) -> bool {
        self.next.is_empty()
    }

    pub fn id(&self) -> String {
        self.id.to_owned()
    }

    pub fn previous(&self) -> Vec<String> {
        self.previous.clone()
    }

    pub fn next(&self) -> Vec<String> {
        self.next.clone()
    }
}

impl<'a, T: PartialEq> Graph<T> {
    pub fn new() -> Self {
        Graph {
            nodes: HashMap::new(),
            visited: Vec::new(),
            sorted: Vec::new(),
            queue: Vec::new(),
        }
    }

    pub fn add_node(&mut self, id: &str, elem: T) {
        let new_node = Node {
            id: id.to_string(),
            next: Vec::new(),
            previous: Vec::new(),
            elem,
        };

        self.nodes.insert(id.to_string(), new_node);
    }

    pub fn add_link(&mut self, from: &str, to: &str) {
        // Check "to" and "from" nodes exist.

        if let Some(from_node) = self.get_node_mut_by_id(from) {
            from_node.next.push(to.to_string())
        }

        if let Some(to_node) = self.get_node_mut_by_id(to) {
            to_node.previous.push(from.to_string())
        }
    }

    pub fn get_node(&self, key: &str) -> NodeResult<T> {
        if let Some(node) = self.nodes.get(key) {
            if self.is_connected(key) {
                return NodeResult::Connected(node);
            }
            NodeResult::Disconnected(node)
        } else {
            NodeResult::NotFound
        }
    }

    pub fn is_connected(&self, key: &str) -> bool {
        matches!(self.get_node(key), NodeResult::Connected(_))
    }

    pub fn get_next(&self, key: &str) -> Option<Vec<String>> {
        match self.get_node(key) {
            NodeResult::Connected(node) => Some(node.next()),
            _ => None,
        }
    }

    pub fn get_previous(&self, key: &str) -> Option<Vec<String>> {
        match self.get_node(key) {
            NodeResult::Connected(node) => Some(node.previous()),
            _ => None,
        }
    }

    pub fn is_merge_node(&self, key: &str) -> bool {
        match self.get_node(key) {
            NodeResult::Connected(node) => node.is_merge(),
            _ => false,
        }
    }

    pub fn is_branch_node(&self, key: &str) -> bool {
        match self.get_node(key) {
            NodeResult::Connected(node) => node.is_branch(),
            _ => false,
        }
    }

    pub fn is_tip_node(&self, key: &str) -> bool {
        match self.get_node(key) {
            NodeResult::Connected(node) => node.is_tip(),
            _ => false,
        }
    }

    // pub fn invalidate_keys(&self, keys: Vec<String>) {}

    pub fn root_node(&self) -> &Node<T> {
        self.nodes.values().find(|node| node.is_root()).unwrap()
    }

    pub fn root_node_key(&self) -> String {
        self.nodes
            .values()
            .find(|node| node.is_root())
            .unwrap()
            .id()
    }

    fn get_node_by_id(&'a self, id: &str) -> Option<&'a Node<T>> {
        self.nodes.get(id)
    }

    fn get_node_mut_by_id(&mut self, id: &str) -> Option<&mut Node<T>> {
        self.nodes.get_mut(id)
    }

    fn dependencies_visited(&self, visited: &[String], node: &Node<T>) -> bool {
        let mut has_dependencies = true;
        for previous_node in node.previous() {
            if !visited.contains(&previous_node) && node.id() != previous_node {
                has_dependencies = false
            }
        }
        has_dependencies
    }

    fn children_visited(&self, visited: &[String], node: &Node<T>) -> bool {
        let mut children_visited = true;
        for next_node in node.next() {
            if !visited.contains(&next_node) {
                children_visited = false
            }
        }
        children_visited
    }

    pub fn get(&self, id: &str) -> Option<&T> {
        self.nodes.get(id).map(|node| &node.elem)
    }

    fn next(&self, visited: &[String], node: &'a Node<T>) -> Option<Vec<String>> {
        let mut next_node_ids = Vec::new();
        for link in &node.next() {
            if !visited.contains(link) {
                next_node_ids.push(link.to_string())
            };
        }
        if next_node_ids.is_empty() {
            return None;
        };
        next_node_ids.reverse();
        Some(next_node_ids)
    }

    pub fn walk(&'a mut self) -> Vec<String> {
        let root_node = self.nodes.values().find(|node| node.is_root()).unwrap();
        let mut queue = vec![root_node];
        let mut visited = Vec::new();

        let push_to_queue = |queue: &mut Vec<&'a Node<T>>, node_id: &str| {
            let node = self.get_node_by_id(node_id).unwrap();
            if !queue.contains(&node) {
                println!("{}: push to queue", node_id);
                queue.push(node)
            };
        };

        let push_to_visited = |visited: &mut Vec<String>, node_id| visited.push(node_id);

        while let Some(mut current_node) = queue.pop() {
            push_to_visited(&mut visited, current_node.id());
            while let Some(mut next_node_ids) = self.next(&visited, current_node) {
                let next_node_id = next_node_ids.pop().unwrap();
                while let Some(node_to_be_queued) = next_node_ids.pop() {
                    push_to_queue(&mut queue, &node_to_be_queued)
                }
                if let Some(next_node) = self.get_node_by_id(&next_node_id) {
                    if next_node.is_merge() {
                        if self.dependencies_visited(&visited, next_node) {
                            println!("{}: is merge and has all dependencies met", next_node.id());
                            push_to_queue(&mut queue, &next_node.id());
                            break;
                        }
                        println!(
                            "{}: is merge and does not have dependencies met",
                            next_node.id()
                        );
                        break;
                    }
                    push_to_visited(&mut visited, next_node.id());
                    current_node = next_node;
                }
            }
        }
        visited
    }
}

#[cfg(test)]
mod test {
    use super::Graph;

    #[test]
    fn basics() {
        let mut graph = Graph::new();
        graph.add_node("a", 1);
        graph.add_node("b", 2);
        graph.add_node("c", 3);
        graph.add_node("d", 3);
        graph.add_node("e", 3);
        graph.add_node("f", 3);
        graph.add_node("g", 3);
        graph.add_node("h", 3);
        graph.add_node("i", 3);
        graph.add_node("j", 3);
        graph.add_node("k", 3);

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
            ["a", "b", "c", "i", "j", "k", "g", "h", "d", "e", "f",]
        )
    }
}
