// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods for expressing p2panda document graphs in mermaid-js

use crate::{
    graph::{Graph, GraphError, Node},
    operation::{AsOperation, OperationWithMeta},
};

/// Trait for parsing a struct into html.
pub trait ToHtml {
    /// Ouput a html string representation of this struct.
    fn to_html(&self) -> String;
}

impl ToHtml for String {
    fn to_html(&self) -> String {
        self.to_string()
    }
}

impl ToHtml for OperationWithMeta {
    fn to_html(&self) -> String {
        let action = match self.operation().action() {
            crate::operation::OperationAction::Create => "CREATE",
            crate::operation::OperationAction::Update => "UPDATE",
            crate::operation::OperationAction::Delete => "DELETE",
        };

        let mut id = String::with_capacity(68);
        id.push_str(self.operation_id().as_str());

        let mut fields = "".to_string();

        for (key, value) in self.fields().unwrap().iter() {
            match value {
                crate::operation::OperationValue::Boolean(_) => todo!(),
                crate::operation::OperationValue::Integer(_) => todo!(),
                crate::operation::OperationValue::Float(_) => todo!(),
                crate::operation::OperationValue::Text(str) => {
                    fields.push_str(&format!("<tr><td>{key}</td><td>{str}</td></tr>"))
                }
                crate::operation::OperationValue::Relation(_) => todo!(),
            }
        }

        format!(r#"<table><tr><td>action</td><td>{action}</td></tr>"#)
            + &format!(r#"<tr><td>hash</td><td>..{}</td></tr>"#, &id[64..])
            + &format!(r#"<tr><td>fields</td><td>{fields}</td></tr></table>"#)
    }
}

/// Build a graph from operations.
pub fn build_graph(operations: &[OperationWithMeta]) -> Graph<OperationWithMeta> {
    let mut graph = Graph::new();

    // Add all operations to the graph.
    for operation in operations {
        graph.add_node(operation.operation_id().as_str(), operation.clone());
    }

    // Add links between operations in the graph.
    for operation in operations {
        if let Some(previous_operations) = operation.previous_operations() {
            for previous in previous_operations {
                graph.add_link(previous.as_str(), operation.operation_id().as_str());
            }
        }
    }
    graph
}

/// A graph node represented as a mermaid string.
pub fn to_mermaid_node<T: PartialEq + Clone + std::fmt::Debug + ToHtml>(node: &Node<T>) -> String {
    format!("{}[{}]", node.key(), node.data().to_html())
}

/// Takes a graph with nodes and converts it into a mermaid string.
pub fn into_mermaid<T: PartialEq + Clone + std::fmt::Debug + ToHtml>(
    graph: Graph<T>,
) -> Result<String, GraphError> {
    let root_node = match graph.root_node() {
        Ok(node) => Ok(node),
        Err(_) => Err(GraphError::NodeNotFound),
    }?;
    let mut queue = vec![root_node];
    let mut sorted_nodes = vec![];
    let mut sorted = vec![];
    let mut graph_tips = vec![];

    let mut mermaid_str = "graph TD;\n".to_string();

    // Pop from the queue while it has items.
    while let Some(mut current_node) = queue.pop() {
        // If the sorted stack is bigger than the number of existing nodes we have a cycle.
        if sorted_nodes.len() > graph.len() {
            return Err(GraphError::CycleDetected);
        }
        // Push this node to the sorted stack...
        sorted_nodes.push(current_node);
        sorted.push(current_node.data());
        if current_node.is_tip() {
            graph_tips.push(current_node.data())
        }

        // ...and then walk the graph starting from this node.
        while let Some(mut next_nodes) = graph.next(&sorted_nodes, current_node) {
            // Pop off the next node we will visit.
            let next_node = next_nodes.pop().expect("Node not in graph");
            mermaid_str += &format!(
                "{} --> {};\n",
                to_mermaid_node(current_node),
                to_mermaid_node(next_node),
            );
            // Write edges to all other nodes connected to this one.
            while let Some(node_to_be_queued) = next_nodes.pop() {
                queue.push(node_to_be_queued);
                mermaid_str += &format!(
                    "{}[{}] --> {}[{}];\n",
                    current_node.key(),
                    current_node.data().to_html(),
                    node_to_be_queued.key(),
                    node_to_be_queued.data().to_html()
                );
            }

            // If it's a merge node, check it's dependencies have all been visited.
            if next_node.is_merge() {
                if graph.dependencies_visited(&sorted_nodes, next_node) {
                    // If they have been, push this node to the queue and exit this loop.
                    queue.push(next_node);

                    break;
                } else if queue.is_empty() {
                    // The queue is empty, but this node has dependencies missing then there
                    // is either a cycle or missing nodes.
                    return Err(GraphError::BadlyFormedGraph);
                }

                break;
            }
            // If it wasn't a merge node, push it to the sorted stack and keep walking.
            sorted_nodes.push(next_node);
            sorted.push(next_node.data());

            // If it is a tip, push it to the graph tips list.
            if next_node.is_tip() {
                graph_tips.push(next_node.data());
            }

            current_node = next_node;
        }
    }
    Ok(mermaid_str)
}

#[cfg(test)]
mod test {

    use crate::{graph::Graph, test_utils::graph::into_mermaid};

    #[test]
    fn into_mermaid_test() {
        let mut graph = Graph::new();

        graph.add_node("a", "A".to_string());
        graph.add_node("b", "B".to_string());
        graph.add_node("c", "C".to_string());
        graph.add_node("d", "D".to_string());
        graph.add_node("e", "E".to_string());
        graph.add_node("f", "F".to_string());
        graph.add_node("g", "G".to_string());
        graph.add_node("h", "H".to_string());
        graph.add_node("i", "I".to_string());

        graph.add_link("a", "b");
        graph.add_link("b", "c");
        graph.add_link("c", "d");
        graph.add_link("d", "e");
        graph.add_link("e", "f");

        graph.add_link("d", "g");
        graph.add_link("g", "h");
        graph.add_link("h", "i");
        graph.add_link("i", "e");

        println!("{}", into_mermaid(graph).unwrap());
    }
}
