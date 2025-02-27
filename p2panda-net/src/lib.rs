// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]

//! `p2panda-net` is a data-type-agnostic p2p networking layer offering robust, direct
//! communication to any device, no matter where they are.
//!
//! It provides a stream-based API for higher layers: Applications subscribe to any "topic" they
//! are interested in and `p2panda-net` will automatically discover similar peers and transport raw
//! bytes between them.
//!
//! Additionally `p2panda-net` can be extended with custom sync protocols for all data types,
//! allowing applications to "catch up on past data", eventually converging to the same state.
//!
//! ## Features
//!
//! Most of the lower-level networking of `p2panda-net` is made possible by the work of
//! [iroh](https://github.com/n0-computer/iroh/) utilising well-established and known standards,
//! like QUIC for transport, (self-certified) TLS for transport encryption, STUN for establishing
//! direct connections between devices, Tailscale's DERP (Designated Encrypted Relay for Packets)
//! for relay fallbacks, PlumTree and HyParView for broadcast-based gossip overlays.
//!
//! p2panda adds crucial functionality on top of iroh for peer-to-peer application development,
//! without tying developers too closely to any pre-defined data types and allowing plenty of space
//! for customisation:
//!
//! 1. Data of any kind can be exchanged efficiently via gossip broadcast ("live mode") or via sync
//!    protocols between two peers ("catching up on past state")
//! 2. Custom network-wide queries to express interest in certain data of applications
//! 3. Ambient peer discovery: Learning about new, previously unknown peers in the network
//! 4. Ambient topic discovery: Learning what peers are interested in, automatically forming
//!    overlay networks per topic
//! 5. Sync protocol API, providing an eventual-consistency guarantee that peers will converge on
//!    the same state over time
//! 6. Manages connections, automatically syncs with discovered peers and re-tries on faults
//! 7. Extension for networks to handle efficient [sync of large
//!    files](https://docs.rs/p2panda-blobs)
//!
//! ## Offline-First
//!
//! This networking crate is designed to run on top of bi-directional, ordered connections on the
//! IP layer (aka "The Internet"), with robustness to work in environments with unstable
//! connectivity or offline time-periods.
//!
//! While this IP-based networking implementation should provide for many "modern" use-cases,
//! p2panda data-types are designed for more extreme scenarios where connectivity can _never_ be
//! assumed and data transmission needs to be highly "delay tolerant": For example "broadcast-only"
//! topologies on top of BLE (Bluetooth Low Energy), LoRa or even Digital Radio Communication
//! infrastructure.
//!
//! ## Extensions
//!
//! `p2panda-net` is agnostic to any data type (sending and receiving raw byte streams) and can
//! seamlessly be extended with external or official p2panda implementations for different parts of
//! the application:
//!
//! 1. Custom Data types exchanged over the network
//! 2. Optional relay nodes to aid connection establishment when peers are behind firewalls etc.
//! 3. Custom sync protocol for any data types, with managed re-attempts on connection failures and
//!    optional re-sync schedules
//! 4. Custom peer discovery strategies (multiple approaches can be used at the same time)
//! 5. Sync and storage of (very) large blobs
//! 6. Fine-tune gossipping behaviour
//! 7. Additional custom protocol handlers
//!
//! ## Integration with other p2panda solutions
//!
//! We provide p2panda's fork-tolerant and prunable append-only logs in `p2panda-core`, offering
//! single-writer and multi-writer streams, authentication, deletion, ordering and more. This can
//! be further extended with an efficient sync implementation in `p2panda-sync` and validation and
//! fast stream-based ingest solutions in `p2panda-streams`.
//!
//! For discovery of peers on the local network, we provide an mDNS-based implementation in
//! `p2panda-discovery`, planned next are additional techniques like "rendesvouz" nodes and random
//! walk algorithms.
//!
//! Lastly we maintain persistance layer APIs in `p2panda-store` for in-memory storage or
//! embeddable, SQL-based databases.
//!
//! In the future we will provide additional implementations for managing access control and group
//! encryption.
//!
//! ## Example
//!
//! ```
//! # use anyhow::Result;
//! use p2panda_core::{PrivateKey, Hash};
//! use p2panda_discovery::mdns::LocalDiscovery;
//! use p2panda_net::{NetworkBuilder, TopicId};
//! use p2panda_sync::TopicQuery;
//! use serde::{Serialize, Deserialize};
//! # #[tokio::main]
//! # async fn main() -> Result<()> {
//!
//! // Peers using the same "network id" will eventually find each other. This is the most global
//! // identifier to group peers into multiple networks when necessary.
//! let network_id = [1; 32];
//!
//! // The network can be used to automatically find and ask other peers about any data the
//! // application is interested in. This is expressed through "network-wide queries" over topics.
//! //
//! // In this example we would like to be able to query messages from each chat group, identified
//! // by a BLAKE3 hash.
//! #[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
//! struct ChatGroup(Hash);
//!
//! impl ChatGroup {
//!     pub fn new(name: &str) -> Self {
//!         Self(Hash::new(name.as_bytes()))
//!     }
//! }
//!
//! impl TopicQuery for ChatGroup {}
//!
//! impl TopicId for ChatGroup {
//!     fn id(&self) -> [u8; 32] {
//!         self.0.into()
//!     }
//! }
//!
//! // Generate an Ed25519 private key which will be used to authenticate your peer towards others.
//! let private_key = PrivateKey::new();
//!
//! // Use mDNS to discover other peers on the local network.
//! let mdns_discovery = LocalDiscovery::new();
//!
//! // Establish the p2p network which will automatically connect you to any discovered peers.
//! let network = NetworkBuilder::new(network_id)
//!     .private_key(private_key)
//!     .discovery(mdns_discovery)
//!     .build()
//!     .await?;
//!
//! // Subscribe to network events.
//! let mut event_rx = network.events().await?;
//!
//! // From now on we can send and receive bytes to any peer interested in the same chat.
//! let my_friends_group = ChatGroup::new("me-and-my-friends");
//! let (tx, mut rx, ready) = network.subscribe(my_friends_group).await?;
//! # Ok(())
//! # }
//! ```
mod addrs;
mod bytes;
pub mod config;
mod engine;
mod events;
pub mod network;
mod protocols;
mod sync;

pub use addrs::{NodeAddress, RelayUrl};
pub use config::Config;
pub use events::SystemEvent;
pub use network::{FromNetwork, Network, NetworkBuilder, RelayMode, ToNetwork};
pub use protocols::ProtocolHandler;
pub use sync::{ResyncConfiguration, SyncConfiguration};

#[cfg(feature = "log-sync")]
pub use p2panda_sync::log_sync::LogSyncProtocol;

/// Unique 32 byte identifier for a network.
///
/// Peers operating on the same network identifier will eventually discover each other. This is the
/// most global identifier to group peers into networks. Different applications may choose to share
/// the same underlying network infrastructure by using the same network identifier.
///
/// Please note that the network identifier should _never_ be the same as any other topic
/// identifier.
pub type NetworkId = [u8; 32];

/// Topic ids are announced on the network and used to identify peers with overlapping interests.
///
/// Once other peers are discovered who are interested in the same topic id, the application will
/// join the gossip overlay under that identifier.
///
/// If an optional sync protocol has been provided, the application will attempt to synchronise
/// past state before entering the gossip overlay.
///
/// ## Designing topic identifiers for applications
///
/// Networked applications, such as p2p systems, usually want to converge to the same state over
/// time so that all users eventually see the same data.
///
/// If we're considering the totality of "all data" the application can create as the "global
/// state", we might want to categorise it into logical "sub-sections", especially when the
/// application gets complex. In an example chat application we might not want to sync _all_ chat
/// group data which has ever been created by all peers, but only a subset of the ones our peer is
/// actually a member of.
///
/// In this case we could separate the application state into distinct topic identifiers, one for
/// each chat group. Now peers can announce their interest in a specific chat group and only sync
/// that particular data.
///
/// ## `TopicQuery` vs. `TopicId`
///
/// Next to topic identifiers p2panda offers a `TopicQuery` trait which allows for even more
/// sophisticated "network queries".
///
/// `TopicId` is a tool for general topic discovery and establishing gossip network overlays.
/// `TopicQuery` is a query for sync protocols to ask for a specific piece of information.
///
/// Consult the `TopicQuery` documentation in `p2panda-sync` for further information.
pub trait TopicId {
    fn id(&self) -> [u8; 32];
}

/// Converts an `iroh` public key type to the `p2panda-core` implementation.
pub(crate) fn to_public_key(key: iroh_base::PublicKey) -> p2panda_core::PublicKey {
    p2panda_core::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}

/// Converts a `p2panda-core` public key to the "iroh" type.
pub(crate) fn from_public_key(key: p2panda_core::PublicKey) -> iroh_base::PublicKey {
    iroh_base::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}

/// Converts a `p2panda-core` private key to the "iroh" type.
pub(crate) fn from_private_key(key: p2panda_core::PrivateKey) -> iroh_base::SecretKey {
    iroh_base::SecretKey::from_bytes(key.as_bytes())
}
