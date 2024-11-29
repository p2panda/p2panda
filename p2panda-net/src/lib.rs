// SPDX-License-Identifier: AGPL-3.0-or-later

//! `p2panda-net` is a data-type-agnostic p2p networking layer which offers robust, direct
//! communication to any device, no matter where they are.
//!
//! It offers gossip overlays for message broadcast, peer identities and stream-based interfaces
//! for your higher application layers. Additionally it comes with modular extensions, for peer and
//! topic discovery, access control or sync protocols with eventual-consistency guarantees.
//!
//! ## Features
//!
//! Most of the lower-level networking of `p2panda-net` is made possible by the work of
//! [Iroh](https://github.com/n0-computer/iroh/) utilising well-established and known standards,
//! like QUIC for transport, STUN for holepunching, Tailscale's DERP (Designated Encrypted Relay
//! for Packets) for relay fallbacks, PlumTree and HyParView for gossipping.
//!
//! `p2panda-net` adds all functionality on top of Iroh we believe is crucial for p2p application
//! development, without tying ourselves too close to any data types or strategies:
//!
//! * Subscription API to topics, independent of if they arrived via gossip ("fast message
//! delivery") or a sync session ("catching up on past state")
//! * Ambient peer discovery, learning about new peers you might not know about
//! * Ambient topic discovery, learning what they are interested in
//! * Sync interfaces, providing the eventual-consistency guarantee that
//! your peers will converge on the same state over time
//! * Queries to ask about specific data in the network
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
