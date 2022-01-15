use std::collections::HashMap;

#[derive(Debug)]
pub struct Graph<T> {
    nodes: HashMap<String, Node<T>>,
    visited: Vec<String>,
    sorted: Vec<String>,
    queue: Vec<String>,
}

#[derive(Debug)]
struct Node<T> {
    id: String,
    elem: T,
    previous: Vec<String>,
    next: Vec<String>,
}

impl<T> Node<T> {
    pub fn is_root(&self) -> bool {
        self.previous.is_empty()
    }

    pub fn is_merge(&self) -> bool {
        self.previous.len() > 1
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

impl<T> Graph<T> {
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

    fn get_node_by_id(&self, id: &str) -> Option<&Node<T>> {
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

    fn next<'a>(&self, visited: &[String], node: &'a Node<T>) -> Option<Vec<String>> {
        let mut next_node_ids = Vec::new();
        for link in &node.next() {
            if !visited.contains(link) {
                next_node_ids.push(link.to_string())
            };
        }
        if next_node_ids.is_empty() {
            return None;
        };
        Some(next_node_ids)
    }

    pub fn walk(&mut self) -> Vec<String> {
        let root_node = self.nodes.values().find(|node| node.is_root()).unwrap();
        let mut queue = vec![root_node];
        let mut visited = Vec::new();
        while let Some(mut current_node) = queue.pop() {
            visited.push(current_node.id());
            while let Some(mut next_node_ids) = self.next(&visited, current_node) {
                let next_node_id = next_node_ids.pop().unwrap();
                while let Some(node_to_be_queued) = next_node_ids.pop() {
                    queue.push(self.get_node_by_id(&node_to_be_queued).unwrap())
                }
                if let Some(next_node) = self.get_node_by_id(&next_node_id) {
                    if next_node.is_merge() {
                        println!("{}: is merge", next_node.id());
                        if self.dependencies_visited(&visited, next_node) {
                            println!("{}: has all dependencies met", next_node.id());
                            queue.push(next_node);
                            break;
                        }
                        println!("{}: does not have dependencies met", next_node.id());
                        break;
                    }
                    visited.push(next_node.id());
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

        graph.add_link("a", "b");
        graph.add_link("b", "c");
        graph.add_link("c", "d");
        graph.add_link("a", "e");
        graph.add_link("e", "f");
        graph.add_link("f", "d");
        graph.add_link("d", "h");
        graph.add_link("h", "g");

        let node_1 = graph.get("a");
        assert_eq!(1, *node_1.unwrap());

        println!("{:#?}", graph.walk());
    }
}
