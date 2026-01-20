// SPDX-License-Identifier: MIT OR Apache-2.0

//! Traits and implementation of p2panda's confidential discovery protocol.
//!
//! ## Motivation
//!
//! Discovery can be used to find nodes and their transport information (to aid establishing a
//! direct peer-to-peer connection) which are interested in the same "topic". A topic in p2panda is
//! a secret, randomly generated hash, similar to a shared symmetric key. Since topics usually
//! represent identifiers or namespaces for data and documents for only a certain amount of people
//! (for example a "text document" or "chat group" or "image folder") it should only be shared with
//! exactly these people and never accidentially leaked in our protocols.
//!
//! With this discovery protocol implementation we are introducing a concrete solution which allows
//! nodes to only ever exchange data when both parties have proven that they are aware of the same
//! topic. No other, unrelated topics will be "leaked" to any party. This is made possible using
//! "Private Equality Testing" (PET) or "Private Set Intersection" which is a secure multiparty
//! computation cryptographic technique. Example:
//!
//! ```text
//! Alice's topics: T1, T2, T3, T4
//! Bob's topics: T1, T4, T5
//!
//! Result after confidential protocol exchange:
//! - Alice will learn from Bob that they are interested in T1 and T4
//! - Bob will learn from Alice that they are interested in T1 and T4
//! ```
//!
//! Having a confidential discovery protocol allows us to build systems where we confidentially
//! sync / exchange data with nodes who have proven that they are aware of this topic. Usually
//! privacy is further improved by keeping an allow list to limit the set of nodes we are
//! establishing direct connections with. This limits with whom we are even exchanging over the
//! discovery protocol. This is handled on the networking layer though (see allow lists in
//! `p2panda-net`) and allows building more private networks of many, explicitly trusted nodes
//! which within can confidentially exchange topics. An example would be a family, team or
//! collective having multiple chat groups - and still nobody should ever learn about what other
//! chat groups exists.
//!
//! To exchange data confidentially we require a direct, authenticated E2EE connection with the
//! remote node and can't move data into DHTs or other globally distributed data types or services.
//! Since every node itself can also serve as an source of new node information we achieve a fully
//! decentralised discovery strategy, making rendezvous nodes or DNS-like lookups redundant.
//!
//! ## Protocol Design
//!
//! Our discovery consists of three systems: A discovery "peer sampling strategy" and the
//! "protocol" itself which is the actual exchange of messages between two nodes and lastly an
//! "address book" which is strictly speaking more of a storage backend to persist and query known
//! node information and associated topics - but important for discovery.
//!
//! For our peer-sampling strategy we're using a basic Random Walk approach: Any random bootstrap
//! node from the address book is selected to initiate depth-first network traversal with frequent
//! resets to allow exploring the network more "broadly" and prevent getting stuck in cycles. The
//! protocol itself is a a simple exchange of salted and hashed topic identifiers, allowing us to
//! confidentially exchange topics beetween two nodes in linear time (growing with the number of
//! topics).
//!
//! Since every topic is a cryptographically secure, randomly generated value, an attacker can not
//! easily learn the underlying value when trying to break the hashing function, even when the
//! hashing function in itself was not designed to prevent such attacks.
//!
//! Bootstrap nodes are a locally configurable set of nodes which are picked first when the
//! discovery process starts.
//!
//! It's also worth mentioning that the random walk can be parallelized in applications (run
//! multiple "walkers" at the same time) and that it is an "ambient" process, constantly running in
//! the background, informing other parts of the stack about confidentially exchanged sets of
//! topics and nodes.
//!
//! ## Inspiration
//!
//! To use Private Equality Testing in this "category" of peer-to-peer protocols was (to our
//! knowledge) first suggested by [Willow](https://willowprotocol.org/) and we believe that
//! this is the only way towards ["more fine"](https://newdesigncongress.org/en/pub/this-is-fine/)
//! peer-to-peer systems.
//!
//! We've heard first about random walk algorithms used as a discovery technique in peer-to-peer
//! systems from the
//! [IPv8](https://py-ipv8.readthedocs.io/en/latest/further-reading/advanced_peer_discovery.html)
//! project.
// TODO: Move address book into `p2panda-store` when crate is ready.
pub mod address_book;
pub mod psi_hash;
#[cfg(feature = "random_walk")]
pub mod random_walk;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
pub mod tests;
pub mod traits;

pub use traits::{DiscoveryProtocol, DiscoveryResult, DiscoveryStrategy};
