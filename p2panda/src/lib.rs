// SPDX-License-Identifier: MIT OR Apache-2.0

// TODO: Remove this later.
#![allow(unused)]

// TODO: Avoid nested error types.
// TODO: Check error type size.

mod builder;
mod network;
pub mod node;
pub mod operation;
pub mod streams;
pub mod topic;

pub use node::Node;
pub use operation::{Extensions, Header, Operation};
pub use topic::Topic;

pub async fn spawn() -> Result<Node, node::NodeError> {
    Node::spawn().await
}

pub fn builder() -> builder::NodeBuilder {
    Node::builder()
}
