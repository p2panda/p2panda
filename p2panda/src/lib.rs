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
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;

pub use node::Node;
pub use operation::{Extensions, Header, Operation};

pub async fn spawn() -> Result<Node, node::NodeError> {
    Node::spawn().await
}

pub fn builder() -> builder::NodeBuilder {
    Node::builder()
}
