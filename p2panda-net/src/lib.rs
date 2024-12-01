// SPDX-License-Identifier: AGPL-3.0-or-later

//! `p2panda-net` is a data-type-agnostic p2p networking layer offering robust, direct
//! communication to any device, no matter where they are.
//!
//! It provides a stream-based API for higher application layers: Applications subscribe to any
//! "topic" they are interested in and `p2panda-net` will automatically discover similar peers and
//! transport raw bytes between them.
//!
//! Additionally `p2panda-net` can be extended with custom sync protocol implementations for all
//! data types, allowing applications to "catch up on past data", eventually converging to the same
//! state.
//!
//! ## Features
//!
//! Most of the lower-level networking of `p2panda-net` is made possible by the work of
//! [iroh](https://github.com/n0-computer/iroh/) utilising well-established and known standards,
//! like QUIC for transport, STUN for establishing direct connections between devices, Tailscale's
//! DERP (Designated Encrypted Relay for Packets) for relay fallbacks, PlumTree and HyParView for
//! broadcast-based gossip overlays.
//!
//! p2panda adds crucial functionality on top of iroh for peer-to-peer application development,
//! without tying developers too close to any pre-defined data types and allowing plenty space for
//! customisation:
//!
//! 1. Data of any kind can be exchanged efficiently via gossip broadcast ("live mode") or via sync
//!    protocols between two peers ("catching up on past state")
//! 2. Custom queries to express interest in certain data of applications
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
//! IP layer (aka "The Internet"), with robustness to work in environments with instable
//! connectivity or offline time-periods.
//!
//! While this IP-based networking implementation should provide for many "modern" use-cases,
//! p2panda data-types are designed for more extreme scenarios where connectivity can _never_ be
//! assumed and data transmission is highly "delay tolerant": For example "broadcast-only"
//! topologies on top of BLE (Bluetooth Low Energy), LoRa or even Digital Radio Communication
//! infrastructure.
//!
//! ## Extensions
//!
//! `p2panda-net` is agnostic to any data type (sending and receiving raw byte streams) and can
//! seamlessly be extended with external or official p2panda implementations. We provide p2panda's
//! fork-tolerant and prunable append-only logs in `p2panda-core`, offering single-writer and
//! multi-writer streams, authentication, deletion, ordering and much more. It can be further
//! extended with an efficient sync implementation in `p2panda-sync` and validation and fast
//! stream-based ingest solutions in `p2panda-streams`. Lastly we maintain persistance layer APIs
//! in `p2panda-store` for in-memory storage or embeddable, SQL-based databases. In the future we
//! will provide additional implementations for managing access control and group encryption.
//!
//! For discovery of peers on the local network, we currently provide an mDNS-based implementation
//! in `p2panda-discovery`, later additional techniques like "rendesvouz" nodes and random walk
//! algorithms.
//!
//! ## Example
//!
//! ```
//! use p2panda_core::{PrivateKey, Hash};
//! use p2panda_net::{NetworkBuilder, Topic, TopicId};
//! use p2panda_discovery::LocalDiscovery;
//! use serde::{Serialize, Deserialize};
//!
//! // All peers knowing the same "network id" will eventually find each other. Use this as the most
//! // global identifier to group peers into multiple networks when necessary. This can be useful if
//! // you're planning to run different applications on top of the same infrastructure.
//! let network_id = b"my-chat-network";
//!
//! // We can use the network now to automatically find and ask other peers about any data we are
//! // interested in. For this we're defining our own "queries" with topics.
//! //
//! // In this example we would like to be able to query messages from each chat, identified by
//! // a BLAKE3 hash.
//! #[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
//! struct ChatChannel(Hash);
//!
//! impl ChatChannel {
//!     pub fn new(name: &str) -> Self {
//!         Self(Hash::new(name.to_bytes()))
//!     }
//! }
//!
//! impl Topic for ChatChannel {}
//!
//! impl TopicId for ChatChannel {
//!     fn id(&self) -> [u8; 32] {
//!         *self.1.as_bytes()
//!     }
//! }
//!
//! // Generate an Ed25519 private key which will be used to identifiy your peer towards others.
//! let private_key = PrivateKey::new();
//!
//! // Use mDNS to discover other peers on the local network.
//! let mdns_discovery = LocalDiscovery::new()?;
//!
//! // Establish the p2p network which will automatically connect you to any peers.
//! let network = NetworkBuilder::new(network_id)
//!     .private_key(private_key)
//!     .discovery(mdns_discovery)
//!     .build()
//!     .await?;
//!
//! // From now on we can send and receive bytes to any peer interested in the same chat channel.
//! let friends_channel = ChatChannel::new("me-and-my-friends");
//! let (tx, mut rx, ready) = network.subscribe(friends_channel).await?;
//! ```
mod addrs;
mod bytes;
pub mod config;
mod engine;
pub mod network;
mod protocols;
mod sync;

pub use addrs::{NodeAddress, RelayUrl};
pub use config::Config;
pub use network::{Network, NetworkBuilder, RelayMode};
pub use protocols::ProtocolHandler;
pub use sync::{ResyncConfiguration, SyncConfiguration};

#[cfg(feature = "log-sync")]
pub use p2panda_sync::log_sync::LogSyncProtocol;

/// A unique 32 byte identifier for a network.
pub type NetworkId = [u8; 32];

/// Topic ids are announced on the network and used to identify peers with overlapping interests.
///
/// Once identified, peers join a gossip overlay and, if a sync protocol has been provided, attempt
/// to synchronize past state before entering "live mode".
pub trait TopicId {
    fn id(&self) -> [u8; 32];
}

pub(crate) fn to_public_key(key: iroh_net::key::PublicKey) -> p2panda_core::PublicKey {
    p2panda_core::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}
