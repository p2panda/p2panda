// SPDX-License-Identifier: MIT OR Apache-2.0

mod builder;
mod forge;
mod network;
pub mod node;
mod offset;
pub mod operation;
mod processor;
pub mod streams;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;

pub use node::Node;
pub use offset::Offset;
pub use operation::{Extensions, Header, Operation};
pub use processor::{Event, EventError, ProcessorStatus};

pub async fn spawn() -> Result<Node, node::NodeError> {
    Node::spawn().await
}

pub fn builder() -> builder::NodeBuilder {
    Node::builder()
}
