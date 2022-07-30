// SPDX-License-Identifier: AGPL-3.0-or-later

//! Generic structs which can be used for building a graph structure and sorting it's nodes in
//! a topological depth-first manner.
//!
//! Graph building API based on [tangle-graph](https://gitlab.com/tangle-js/tangle-graph) and graph
//! sorting inspired by [incremental-topo](https://github.com/declanvk/incremental-topo).
//!
//! The unique character in this implementation is that the graph sorting is deterministic, with
//! the paths chosen to walk first being decided by a > comparison between the data contained in
//! each node. If two graphs contain the same nodes and links, regardless to the order they were
//! added, the final sorting will be the same.
//!
//! ## Example
//!
//! ```
//! # extern crate p2panda_rs;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use p2panda_rs::graph::Graph;
//!
//! // Instantiate the graph.
//!
//! let mut graph = Graph::new();
//!
//! // Add some nodes to the graph.
//!
//! graph.add_node(&'a', 'A');
//! graph.add_node(&'b', 'B');
//! graph.add_node(&'c', 'C');
//! graph.add_node(&'d', 'D');
//! graph.add_node(&'e', 'E');
//! graph.add_node(&'f', 'F');
//! graph.add_node(&'g', 'G');
//! graph.add_node(&'h', 'H');
//!
//! // Add some links between the nodes.
//!
//! graph.add_link(&'a', &'b');
//! graph.add_link(&'b', &'c');
//! graph.add_link(&'c', &'d');
//! graph.add_link(&'d', &'e');
//! graph.add_link(&'e', &'f');
//! graph.add_link(&'a', &'g');
//! graph.add_link(&'g', &'h');
//! graph.add_link(&'h', &'d');
//!
//! // The graph looks like this:
//! //
//! //  /--[B]<--[C]--\
//! // [A]<--[G]<-----[H]<--[D]
//!
//! // We can sort it topologically.
//!
//! let sorted = graph.sort()?.sorted();
//!
//! assert_eq!(sorted, vec!['A', 'B', 'C', 'G', 'H', 'D', 'E', 'F']);
//!
//! // Or done more poetically:
//! let mut graph = Graph::new();
//! graph.add_node(&'a', "Wake Up".to_string());
//! graph.add_node(&'b', "Make Coffee".to_string());
//! graph.add_node(&'c', "Drink Coffee".to_string());
//! graph.add_node(&'d', "Stroke Cat".to_string());
//! graph.add_node(&'e', "Look Out The Window".to_string());
//! graph.add_node(&'f', "Start The Day".to_string());
//! graph.add_node(&'g', "Cat Jumps Off Bed".to_string());
//! graph.add_node(&'h', "Cat Meows".to_string());
//! graph.add_node(&'i', "Brain Receives Caffeine".to_string());
//! graph.add_node(&'j', "Brain Starts Engine".to_string());
//! graph.add_node(&'k', "Brain Starts Thinking".to_string());
//! graph.add_link(&'a', &'b');
//! graph.add_link(&'b', &'c');
//! graph.add_link(&'c', &'d');
//! graph.add_link(&'d', &'e');
//! graph.add_link(&'e', &'f');
//!
//! graph.add_link(&'a', &'g');
//! graph.add_link(&'g', &'h');
//! graph.add_link(&'h', &'d');
//! graph.add_link(&'c', &'i');
//! graph.add_link(&'i', &'j');
//! graph.add_link(&'j', &'k');
//! graph.add_link(&'k', &'f');
//!
//! // The graph looks like this:
//! //
//! // ["Cat Jumps Off Bed"]-->["Wake Up"]
//! //   ^                       ^
//! //   |                     ["Make Coffee"]
//! //   |                       ^
//! //   |                     ["Drink Coffee"]<-------["Brain Receives Caffeine"]
//! //   |                       ^                       ^
//! // ["Cat Meows"]<----------["Stroke Cat"]          ["Brain Starts Engine"]
//! //                           ^                       ^
//! //                         ["Look Out The Window"]   |
//! //                           ^                       |
//! //                         ["Start The Day"]------>["Brain Starts Thinking"]
//! //
//!
//! assert_eq!(
//!     graph.walk_from(&'a')?.sorted(),
//!     [
//!         "Wake Up".to_string(),
//!         "Make Coffee".to_string(),
//!         "Drink Coffee".to_string(),
//!         "Brain Receives Caffeine".to_string(),
//!         "Brain Starts Engine".to_string(),
//!         "Brain Starts Thinking".to_string(),
//!         "Cat Jumps Off Bed".to_string(),
//!         "Cat Meows".to_string(),
//!         "Stroke Cat".to_string(),
//!         "Look Out The Window".to_string(),
//!         "Start The Day".to_string()
//!     ]
//! );
//!
//! # Ok(())
//! # }
//! ```
pub mod error;
#[allow(clippy::module_inception)]
mod graph;

pub use graph::Graph;
