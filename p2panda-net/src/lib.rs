// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data-type-agnostic p2p networking, discovery, gossip and local-first sync.
//!
//! ## Features
//!
//! `p2panda-net` is a collection of Rust modules providing solutions for a whole set of
//! peer-to-peer and [local-first] application requirements. Collectively these modules solve the
//! problem of event delivery.
//!
//! - [Publish & Subscribe] for ephemeral messages (gossip protocol)
//! - Publish & Subscribe for messages with [Eventual Consistency] guarantee (sync protocol)
//! - Confidentially discover nodes who are interested in the same topic ([Private Set Intersection])
//! - Establish and manage direct connections to any device over the Internet (using [iroh])
//! - Monitor system with supervisors and restart modules on critical failure (Erlang-inspired
//!   [Supervision Trees])
//! - Modular API allowing users to choose or replace the layers they want to use
//!
//! ## Important note
//!
//! `p2panda-net` depends on a fixed version
//! ([`#117`](https://github.com/n0-computer/iroh-gossip/pull/117)) of `iroh-gossip` which has not
//! been published yet.
//!
//! Please patch your build with the fixed crate for now until this is handled upstream by adding
//! the following lines to your root `Cargo.toml`:
//!
//! ```toml
//! [patch.crates-io]
//! iroh-gossip = { git = "https://github.com/p2panda/iroh-gossip", rev = "533c34a2758518ece19c1de9f21bc40d61f9b5a5" }
//! ```
//!
//! ## Getting Started
//!
//! Install the Rust crate using `cargo add p2panda-net`.
//!
//! ```rust
//! # use std::error::Error;
//! #
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn Error>> {
//! use futures_util::StreamExt;
//! use p2panda_core::Hash;
//! use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
//! use p2panda_net::{AddressBook, Discovery, Endpoint, MdnsDiscovery, Gossip};
//!
//! // Topics are used to discover other nodes and establish connections around them.
//! let topic = Hash::new(b"shirokuma-cafe").into();
//!
//! // Maintain an address book of newly discovered or manually added nodes.
//! let address_book = AddressBook::builder().spawn().await?;
//!
//! // Establish direct connections to any device with the help of iroh.
//! let endpoint = Endpoint::builder(address_book.clone())
//!     .spawn()
//!     .await?;
//!
//! // Discover nodes on your local-area network.
//! let mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
//!     .mode(MdnsDiscoveryMode::Active)
//!     .spawn()
//!     .await?;
//!
//! // Confidentially discover nodes interested in the same topic.
//! let discovery = Discovery::builder(address_book.clone(), endpoint.clone())
//!     .spawn()
//!     .await?;
//!
//! // Disseminate messages among nodes.
//! let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
//!     .spawn()
//!     .await?;
//!
//! // Join topic to publish and subscribe to stream of (ephemeral) messages.
//! let cafe = gossip.stream(topic).await?;
//!
//! // This message will be seen by other nodes if they're online. If you want messages to arrive
//! // eventually, even when they've been offline, you need to use p2panda's "sync" module.
//! cafe.publish(b"Hello, Panda!").await?;
//!
//! let mut rx = cafe.subscribe();
//! tokio::spawn(async move {
//!     while let Some(Ok(bytes)) = rx.next().await {
//!         println!("{}", String::from_utf8(bytes).expect("valid UTF-8 string"));
//!     }
//! });
//! #
//! # Ok(())
//! # }
//! ```
//!
//! For a complete command-line application using `p2panda-net` with a sync protocol, see our
//! [`chat.rs`] example.
//!
//! ## Local-first Event Delivery
//!
//! `p2panda-net` is concerned with the **event delivery** layer of peer-to-peer and local-first
//! application stacks.
//!
//! This layer ensures that your application's data eventually arrives on all devices in a
//! peer-to-peer fashion, no matter where they are and if they've been offline.
//!
//! ## Decentralised and offline-first
//!
//! `p2panda-net` is designed for ad-hoc network topologies with **no central registry** and where
//! the size of the network might be unknown. Nodes can go on- or offline at any point in time.
//!
//! ## Broadcast topology
//!
//! The [Publish & Subscribe] methods in `p2panda-net` suggest a broadcast topology, where one node
//! can communicate to a whole group by sending a single message.
//!
//! Reducing the API surface of direct connections helps with building a wide range of peer-to-peer
//! applications which do not require knowledge of stateful connections but rather look for **state
//! convergence**. This aligns well with the **local-first** paradigm.
//!
//! This approach is a prerequisite to make applications compatible with genuine broadcast-based
//! communication systems, like **LoRa, Bluetooth Low Energy (BLE) or packet radio**.
//!
//! `p2panda-net` can be understood as a broadcast abstraction independent of the underlying
//! transport, including the Internet Protocol or other stateful connection protocols where
//! underneath we're still maintaining connections.
//!
//! ## Bring your own Data-Type
//!
//! `p2panda-net` is agnostic over the actual data of your application. It can be encoded in any
//! way (JSON, CBOR, etc.) and hold any data you need (CRDTs, messages, etc.).
//!
//! Your choice of sync protocol will determine a concrete **Base Convergent Data Type** (Base
//! CDT). The data type must be known in order to design efficient sync protocols. However, these
//! data types are simply carriers for your own application data, regardless of what form it takes.
//!
//! If you're interested in bringing your own Base CDT (for example Merkle-Trees or Sets) we have
//! lower-level APIs and traits in [`p2panda-sync`] which allow you to implement your own sync
//! protocol next to the rest of `p2panda-net`.
//!
//! ## Modules
//!
//! All modules can be enabled by feature flags, most of them are enabled by default.
//!
//! ### Direct Internet connections with iroh [`Endpoint`]
//!
//! Most of the lower-level Internet Protocol networking of `p2panda-net` is made possible by the
//! work of [iroh] utilising well-established and known standards, like QUIC for transport,
//! (self-certified) TLS 1.3 for transport encryption, QUIC Address Discovery (QAD) for STUN and
//! TURN servers for relayed fallbacks.
//!
//! ### Node and Confidental Topic [`Discovery`]
//!
//! Our random-walk discovery algorithm finds other nodes in the network without any centralised
//! registry. Any node can serve as a **bootstrap** into the network.
//!
//! `p2panda-net` is designed around topics of shared interest and we need an additional topic
//! discovery strategy to find nodes sharing the same topics.
//!
//! Topics usually represent identifiers or namespaces for data and documents associated with a
//! specific group of people (for example a text document, chat group or image folder). For this
//! reason, a topic should never be leaked to people outside of the intended group, whether
//! accidentally or purposefully.
//!
//! Read more about how we've implemented confidential topic discovery (and thus sync) in
//! [`p2panda-discovery`].
//!
//! ### Ephemeral Messaging via [`Gossip`] Protocol
//!
//! Not all messages in peer-to-peer applications need to be persisted, for example cursor
//! positions or "awareness & presence" status.
//!
//! For these use-cases `p2panda-net` offers a gossip protocol to broadcast ephemeral messages to
//! all online nodes interested in the same topic.
//!
//! ### Eventual Consistent local-first [`LogSync`]
//!
//! In local-first applications we want to converge towards the same state eventually, which
//! requires nodes to catch up on missed messages - independent of if they've been offline or
//! not.
//!
//! `p2panda-net` comes with a default `LogSync` protocol implementation which uses p2panda's
//! **append-only log** Base Convergent Data Type (CDT).
//!
//! After initial sync finished, nodes switch to **live-mode** to directly push new messages to the
//! network using a gossip protocol.
//!
//! ### Local [`MdnsDiscovery`] finding nearby devices
//!
//! Some devices might be already reachable on your local-area network where no Internet will be
//! required to connect to them. mDNS discovery helps with finding these nodes.
//!
//! ### Manage nodes and associated topics in [`AddressBook`]
//!
//! To keep track of all nodes and their topic interests we're managing a local and persisted
//! address book.
//!
//! The address book is an important tool to watch for transport information changes, keep track of
//! stale nodes and **identify network partitions** which can be automatically "healed".
//!
//! Use the address book to manually add nodes to bootstrap a network from.
//!
//! ### Robust, failure-resistant modules with [`Supervisor`]
//!
//! All modules in `p2panda-net` are internally implemented with the [Actor Model]. Inspired by
//! Erlang's [Supervision Trees] these actors can be monitored and automatically restarted on
//! critical failure (caused by bugs in our code or third-party dependencies).
//!
//! Use the `supervisor` flag to enable this feature.
//!
//! ## Security & Privacy
//!
//! Every connection attempt to any node in a network can reveal sensitive meta-data, for example
//! IP addresses or knowledge of the network or data itself.
//!
//! With `p2panda-net` we work towards a best-effort in containing accidental leakage of such
//! information by:
//!
//! - Use **Network identifiers** to actively partition the network. The identifier serves as a
//!   shared secret; nodes will not be able to establish connections if their identifiers differ.
//! - Use **Confidential Discovery and Sync** to only reveal information about ourselves and
//!   exchange application data with nodes who have knowledge of the same topic.
//! - **Disable mDNS** discovery by default to avoid unknowingly leaking information in local-area
//!   networks.
//! - Give full control over which **boostrap nodes** and **STUN / TURN / relay servers** to
//!   choose. They aid with establishing connections and overlays and can acquire more knowledge over
//!   networking behaviour than other participants.
//! - Allow **connecting to nodes** without any intermediaries, which unfortunately is only
//!   possible if the address is directly reachable.
//!
//! In the future we're planning additional features to improve privacy:
//!
//! - Support **"onion" routing protocols** (Tor, I2P, Veilid) and mix-networks (Katzenpost) to
//!   allow multi-hop routing without revealing the origin of the sender. [`#934`]
//! - Introduce **Allow- and Deny-lists** for nodes to give fine-grained access with whom we can
//!   ever form a connection with. This can be nicely paired with an access control system. [`#925`]
//!
//! [local-first]: https://www.inkandswitch.com/local-first-software/
//! [Publish & Subscribe]: https://en.wikipedia.org/wiki/Publish%E2%80%93subscribe_pattern
//! [Eventual Consistency]: https://en.wikipedia.org/wiki/Eventual_consistency
//! [Actor Model]: https://en.wikipedia.org/wiki/Actor_model
//! [Private Set Intersection]: https://en.wikipedia.org/wiki/Private_set_intersection
//! [Supervision Trees]: https://adoptingerlang.org/docs/development/supervision_trees/
//! [iroh]: https://www.iroh.computer/
//! [`chat.rs`]: https://github.com/p2panda/p2panda/blob/main/p2panda-net/examples/chat.rs
//! [`#925`]: https://github.com/p2panda/p2panda/issues/925
//! [`#934`]: https://github.com/p2panda/p2panda/issues/934
//! [`p2panda-discovery`]: https://docs.rs/p2panda-discovery/latest/p2panda_discovery/
//! [`p2panda-sync`]: https://docs.rs/p2panda-discovery/latest/p2panda_sync/
#[cfg(feature = "address_book")]
pub mod address_book;
pub mod addrs;
pub mod cbor;
#[cfg(feature = "discovery")]
pub mod discovery;
#[cfg(feature = "gossip")]
pub mod gossip;
#[cfg(feature = "iroh_endpoint")]
pub mod iroh_endpoint;
#[cfg(feature = "iroh_mdns")]
pub mod iroh_mdns;
#[cfg(feature = "supervisor")]
pub mod supervisor;
#[cfg(feature = "sync")]
pub mod sync;
#[cfg(any(test, feature = "test_utils"))]
#[doc(hidden)]
pub mod test_utils;
pub mod timestamp;
pub mod utils;
pub mod watchers;

#[cfg(feature = "address_book")]
#[doc(inline)]
pub use address_book::AddressBook;
#[cfg(feature = "discovery")]
#[doc(inline)]
pub use discovery::Discovery;
#[cfg(feature = "gossip")]
#[doc(inline)]
pub use gossip::Gossip;
#[cfg(feature = "iroh_endpoint")]
#[doc(inline)]
pub use iroh_endpoint::Endpoint;
#[cfg(feature = "iroh_mdns")]
#[doc(inline)]
pub use iroh_mdns::MdnsDiscovery;
#[cfg(feature = "supervisor")]
#[doc(inline)]
pub use supervisor::Supervisor;
#[cfg(feature = "sync")]
#[doc(inline)]
pub use sync::LogSync;

/// Identifer for a node in the network.
///
/// Each node in p2panda has a unique identifier created as a cryptographic key to globally
/// identify it and encrypt network traffic for this node only.
pub type NodeId = p2panda_core::PublicKey;

/// Identifier for an gossip- or sync topic.
///
/// A topic identifier is required when subscribing or publishing to a stream.
///
/// Topics usually describe concrete data which nodes want to exchange over, for example a document
/// id or chat group id and so forth. Applications usually want to share topics via a secure side
/// channel.
///
/// **WARNING:** Sensitive topics have to be treated like secret values and generated using a
/// cryptographically secure pseudorandom number generator (CSPRNG). Otherwise they can be easily
/// guessed by third parties or leaked during discovery.
pub type TopicId = [u8; 32];

/// Identifier for a network.
///
/// The network identifier is used to achieve separation and prevent interoperability between
/// distinct networks. This is the most global identifier to group nodes into networks. Different
/// applications may choose to share the same underlying network infrastructure by using the same
/// network identifier.
///
/// A BLAKE3 hash function is performed against each protocol identifier which is registered with
/// `p2panda-net`, using the network identifier as an additional input. Even if two instances of
/// `p2panda-net` are created with the same network protocols, any communication attempts will fail
/// if they are not using the same network identifier.
///
/// **WARNING:** Sensitive network identifiers have to be treated like secret values and generated
/// using a cryptographically secure pseudorandom number generator (CSPRNG). Otherwise they can be
/// easily guessed by third parties.
pub type NetworkId = [u8; 32];

/// Identifier for a protocol.
///
/// A BLAKE3 hash function is performed against each protocol identifier which is registered with
/// `p2panda-net`, using the network identifier as an additional input. Even if two instances of
/// `p2panda-net` are created with the same network protocols, any communication attempts will fail
/// if they are not using the same network identifier.
pub type ProtocolId = Vec<u8>;

/// Default network identifier used by `p2panda-net` for all transports.
///
/// See [NetworkId] for configuring your custom identifier to opt-out of the default network.
pub const DEFAULT_NETWORK_ID: NetworkId = [
    247, 69, 248, 242, 132, 120, 159, 230, 98, 100, 214, 200, 78, 40, 79, 94, 174, 8, 12, 27, 84,
    195, 246, 159, 132, 240, 79, 208, 1, 43, 132, 118,
];

/// Hash the concatenation of the given protocol- and network identifiers.
fn hash_protocol_id_with_network_id(
    protocol_id: impl AsRef<[u8]>,
    network_id: NetworkId,
) -> Vec<u8> {
    p2panda_core::Hash::new([protocol_id.as_ref(), &network_id].concat())
        .as_bytes()
        .to_vec()
}
