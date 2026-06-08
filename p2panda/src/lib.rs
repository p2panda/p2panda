// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(docsrs, feature(doc_cfg))]

//! p2panda's high-level Node API is an opinionated, out-of-the-box peer-to-peer stack which
//! orchestrates all individual [p2panda] modules.
//!
//! ```rust
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let topic = p2panda::Topic::random();
//! let node = p2panda::spawn().await?;
//! let (tx, rx) = node.stream(topic).await?;
//! # tx.publish(b"Hello!".to_vec()).await?;
//! # Ok(())
//! # }
//! ```
//!
//! It provides peer-to-peer networking, discovery, bootstrap, local-first sync, event streaming,
//! causal ordering, storage, and more in one easy-to-use API.
//!
//! ## Features
//!
//! - High-level p2panda Node API for building decentralised p2p and [local-first] applications with
//!   minimal setup
//! - Unified orchestration of p2p networking, node discovery, mDNS, bootstrap, [eventually
//!   consistent] sync, event streaming, causal ordering, pruning and persistence
//! - Topic-based [Publish & Subscribe] model with partial replication - sync only the data relevant
//!   to a topic
//! - Transport-agnostic event delivery architecture supporting Internet p2p today (QUIC/iroh) and
//!   future mesh/radio transports such as BLE and LoRa
//! - Built on single-writer append-only, fork-tolerant CRDT operation logs with pruning,
//!   multi-writer causal ordering, and efficient sync
//! - Persistent local SQLite storage for operations, sync state, address books, stream cursors, and
//!   soon encryption/access-control state
//! - Event-stream-inspired consumer model with acknowledgements, replay support, at-least-once
//!   delivery semantics and crash recovery
//! - Atomic transactional processing pipeline for resilience against crashes and corrupted database
//!   state
//! - Observable system state, events, and metrics
//!
//! ## Walkaway Stack
//!
//! The Node API is designed around a separation between the event delivery and event processing
//! layers. Applications built with p2panda should not need to care abot _where_ messages originate
//! from, but rather _how_ they are processed.
//!
//! The stack confidentially discovers nodes interested in the same topic, synchronises missed
//! messages and delivers them to the processing layer.
//!
//! Today this is implemented over the Internet using [iroh] for direct peer-to-peer connections
//! but the abstraction is designed to support additional transports such as LoRa or BLE, including
//! delay-tolerant and store-and-forward mesh network topologies in the future.
//!
//! Application developers primarily interact with the API to monitor networking and sync activity,
//! configure transports and manage access control, while the stack handles message delivery,
//! synchronisation and persistence.
//!
//! This allows applications to remain portable across different network infrastructures - what we
//! refer to as a ["Walkaway Stack"].
//!
//! ### Append-only log operations
//!
//! Application messages are transported using p2panda's `Operation` data type: a single-writer
//! append-only log with pruning support and fork resistance. Multiple logs can coexist
//! independently or form causally-ordered graphs when arranged in multi-writer streams over a
//! topic.
//!
//! Append-only logs are also well-suited for radio-based meshes, where synchronisation needs to
//! remain bandwidth-efficient. State vectors can be exchanged in constant size and work naturally
//! in broadcast-oriented networks.
//!
//! Operations can be thought of as carriers for application data, similar to how IP datagrams
//! transport arbitrary payloads over the Internet.
//!
//! ### Composable event processors
//!
//! Additional system-level functionality can be layered on top of Operations by extending their
//! headers with metadata for pruning, tombstones, causal ordering, group encryption and other
//! behaviours.
//!
//! The design also allows integration with external processors, CRDTs, key agreement solutions,
//! databases, schemas or capability systems such as Willow's [Meadowcap] or [UCAN] for permission
//! checks and delegated access control.
//!
//! ### Multicast by nature
//!
//! Applications generate a Topic (random bytes) and publish messages into it while subscribing to
//! receive messages from other nodes. This follows a [Publish & Subscribe] (PubSub) model.
//!
//! Topics provide a way to group related data. A topic might represent a text document, a chess
//! game session or a chat room for example.
//!
//! ### SQLite database
//!
//! The Node API persists synchronised Operations in a local SQLite database, together with
//! additional state such as address books, causal ordering buffers, topic mappings and stream
//! cursors.
//!
//! Future versions may also store encryption state, secret key material and access control
//! metadata.
//!
//! ### Event streaming and state materialisation
//!
//! Once data is available locally, the stack processes and delivers it to the application layer
//! using concepts inspired by event-streaming systems such as NATS JetStream or Kafka - adapted for
//! decentralised and autonomous p2p networks.
//!
//! Subscribing to a topic creates a stateful stream consumer.
//!
//! Each operation first passes through an internal _system-level_ processing layer where the system
//! validates log integrity, applies pruning rules, performs causal ordering and, in the future,
//! handles decryption.
//!
//! The processed operation is then forwarded to the _application-level_ together with metadata and
//! debugging information.
//!
//! Applications can then apply their own domain-specific processing logic, such as updating a
//! document title, processing a chat message, applying a text CRDT operation or handling a chess
//! move.
//!
//! ### Crash resilience & acknowledgements
//!
//! Operations can be acknowledged after successful processing. Once acknowledged, they are
//! internally marked as processed and will not be delivered again when re-subscribing to the same
//! stream.
//!
//! By default acknowledgements are handled automatically, though manual control is also possible
//! for advanced use cases.
//!
//! This becomes important in scenarios where an application crashes or is terminated during
//! processing - for example on mobile devices. Unacknowledged operations are replayed on the next
//! startup, allowing applications to maintain at-least-once processing guarantees.
//!
//! Internally, processing and state updates are performed atomically using transactions to minimise
//! the risk of corruption during unexpected shutdowns. Combined with acknowledgements and replay
//! support, this provides a resilient foundation for long-running p2p applications.
//!
//! Replay functionality can also be used to rebuild application state after schema changes, logic
//! updates or recovery scenarios.
//!
//! ### Local-first publishing
//!
//! Publishing a message appends a new operation to the right log for the given topic, signs it,
//! persists it locally and forwards it to the networking layer.
//!
//! If nodes are currently connected, operations may be propagated eagerly in real time. Otherwise,
//! missed operations are synchronised automatically the next time nodes are online again.
//!
//! ## Examples
//!
//! ### Topic stream
//!
//! ```rust,no_run
//! # use futures_util::StreamExt;
//! # use p2panda::Topic;
//! # use p2panda::streams::StreamEvent;
//! #
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Spawn default, in-memory node with mdns discovery.
//! let node = p2panda::spawn().await?;
//!
//! // Generate random topic.
//! let chat_id = Topic::random();
//!
//! // Publish and subscribe topic stream with sync.
//! let (tx, mut rx) = node.stream(chat_id).await?;
//! tx.publish("Hello, Panda!".to_string()).await?;
//!
//! while let Some(event) = rx.next().await {
//!     // React to system events.
//!     if let StreamEvent::SyncStarted {
//!         remote_node_id,
//!         incoming_bytes,
//!         ..
//!     } = event {
//!         println!("syncing {} bytes from {}", incoming_bytes, remote_node_id);
//!     }
//!
//!     // Handle application messages.
//!     if let StreamEvent::Processed { operation, .. } = event {
//!         println!(
//!             "[{}]: received {} from {}",
//!             operation.timestamp(),
//!             operation.message(),
//!             operation.author(),
//!         );
//!     }
//! }
//! #
//! # Ok(())
//! # }
//! ```
//!
//! ### Node configuration
//!
//! ```rust,no_run
//! # use p2panda::RelayUrl;
//! #
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Public key of node helping to bootstrap the network for any topic.
//! let bootstrap_id = "1021253b4ccdfba6cac196d2b9fa6ebb605b747bcd502d6bf1d46887f04ec913".parse()?;
//!
//! // URL of iroh relay to establish a direct connection over the Internet.
//! let relay_url: RelayUrl = "https://my.relay.link".parse()?;
//!
//! // Persist SQLite database under given file path.
//! let database_url = "db.sqlite";
//!
//! // Check the builder docs for more information on configuration options.
//! let node = p2panda::builder()
//!     .database_url(database_url)
//!     .bootstrap(bootstrap_id, relay_url.clone())
//!     .relay_url(relay_url)
//!     .spawn()
//!     .await?;
//! #
//! # Ok(())
//! # }
//! ```
//!
//! ### Application messages
//!
//! ```rust
//! # use p2panda::Topic;
//! # use serde::{Serialize, Deserialize};
//! #
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // This will automatically serialise to CBOR.
//! #[derive(Serialize, Deserialize)]
//! enum CalendarMessage {
//!     Update { title: String },
//!     Delete,
//! }
//!
//! let node = p2panda::spawn().await?;
//! let topic = Topic::random();
//! let (tx, rx) = node.stream::<CalendarMessage>(topic).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ["Walkaway Stack"]: https://fosdem.org/2026/schedule/event/J3FLC3-walkaway-stack/
//! [Meadowcap]: https://willowprotocol.org/specs/meadowcap/index.html
//! [Publish & Subscribe]: https://en.wikipedia.org/wiki/Publish%E2%80%93subscribe_pattern
//! [UCAN]: https://ucan.xyz/
//! [eventually consistent]: https://en.wikipedia.org/wiki/Eventual_consistency
//! [iroh]: https://www.iroh.computer/
//! [local-first]: https://www.inkandswitch.com/local-first-software/
//! [p2panda]: https://p2panda.org
mod builder;
pub mod credentials;
mod forge;
pub mod network;
pub mod node;
pub mod operation;
pub mod processor;
pub mod streams;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;

// Useful external types we want to re-export for convenience.
#[doc(no_inline)]
pub use p2panda_core::{Cursor, Hash, SigningKey, Topic, VerifyingKey};
#[doc(no_inline)]
pub use p2panda_net::iroh_endpoint::{EndpointAddr, RelayUrl};
#[doc(no_inline)]
pub use p2panda_net::{NetworkId, NodeId};

pub use builder::NodeBuilder;
pub use credentials::Credentials;
#[doc(inline)]
pub use node::Node;

/// Spawns a `Node` using default configuration parameters.
pub async fn spawn() -> Result<Node, node::SpawnError> {
    Node::spawn().await
}

/// Returns the builder for a `Node`.
pub fn builder() -> builder::NodeBuilder {
    Node::builder()
}

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_spawn() {
        let runtime = tokio::runtime::Runtime::new().unwrap();

        runtime.spawn(async move {
            let builder = crate::Node::builder();
            builder.spawn().await.unwrap();
        });
    }
}
