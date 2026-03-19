// SPDX-License-Identifier: MIT OR Apache-2.0

mod builder;
mod forge;
mod network;
pub mod node;
pub mod operation;
pub mod processor;
pub mod streams;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;

pub use builder::NodeBuilder;
#[doc(inline)]
pub use node::Node;

pub async fn spawn() -> Result<Node, node::SpawnError> {
    Node::spawn().await
}

pub fn builder() -> builder::NodeBuilder {
    Node::builder()
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_runtime_spawn() {
        let runtime = tokio::runtime::Runtime::new().unwrap();

        runtime.spawn(async move {
            let builder = crate::Node::builder();
            builder.spawn().await.unwrap();
        });
    }
}
