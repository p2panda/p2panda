// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]

//! Core data types used across the p2panda stack to offer distributed, secure and efficient data
//! transfer between peers.
//!
//! The main data type is a highly extensible, cryptographically secure append-only log
//! implementation. It provides all the basic features required to implement more advanced
//! distributed data types commonly required when building peer-to-peer and local-first
//! applications.
//!
//! ## Features
//!
//! - Cryptographic signatures for authorship verification and tamper-proof messages
//! - Authors can maintain one or many logs
//! - Single-writer logs which can be combined to support multi-writer collaboration
//! - Compatible with any application data and CRDT
//! - Various ordering algorithms
//! - Supports efficient, partial sync
//! - Compatible with any networking scenario (even broadcast-only, for example for packet radio)
//! - Fork-tolerant
//! - Pruning of outdated messages
//! - Highly extensible with custom features, for example prefix-deletion, ephemeral
//!   "self-destructing" messages, etc.
//!
//! p2panda logs are made up of [`Operation`]s. Authors sign operations using their cryptographic
//! key pair and append them to a log. An author may have one or many logs. The precise means of
//! identifying logs is not defined by this crate (see extensions).
//!
//! A common challenge in distributed systems is how to order operations written concurrently by
//! different authors and/or processes. Operations contain information which can be used for
//! establishing order depending on one's use case:
//! - `timestamp`: UNIX timestamp describing when the operation was created.
//! - `previous`: List of hashes referring to the previously observed operations to establish
//!   cryptographically secure partial-ordering.
//!
//! Custom extension fields can be defined by users of this library to introduce additional
//! functionality depending on their particular use cases. p2panda provides our own extensions
//! which are required when using our other crates offering more advanced functionality needed for
//! application building (CRDTs, access control, encryption, ephemeral data, garbage collection,
//! etc.), but it's entirely possible for users to define their own extensions as well.
//!
//! An operation is constructed from a [`Header`] and a [`Body`], the `Header` contains all
//! metadata associated with the particular operation, and the `Body` contains the actual
//! application message bytes. This allows "off-chain" handling, where the important bits in the
//! headers are transmitted via an prioritised channel and secondary information can be loaded
//! "lazily". Additionally it allows deletion of payloads without breaking the integrity of the
//! append-only log.
//!
//! ## Example
//!
//! ```
//! use p2panda_core::{Body, Header, Operation, PrivateKey};
//!
//! // Every operation is cryptographically authenticated by an author by signing it with an
//! // Ed25519 key pair. This method generates a new private key for us which needs to be securely
//! // stored for re-use.
//! let private_key = PrivateKey::new();
//!
//! // Operations consist of an body (with the actual application data) and a header,
//! // enhancing the data to be used in distributed networks.
//! let body = Body::new("Hello, Sloth!".as_bytes());
//! let mut header = Header {
//!     version: 1,
//!     public_key: private_key.public_key(),
//!     signature: None,
//!     payload_size: body.size(),
//!     payload_hash: Some(body.hash()),
//!     timestamp: 1733170247,
//!     seq_num: 0,
//!     backlink: None,
//!     previous: vec![],
//!     extensions: (),
//! };
//!
//! // Sign the header with the author's private key. From now on it's ready to be sent!
//! header.sign(&private_key);
//! ```
pub mod cbor;
pub mod extensions;
pub mod hash;
pub mod identity;
pub mod operation;
#[cfg(feature = "prune")]
pub mod prune;
mod serde;

pub use extensions::{Extension, Extensions};
pub use hash::{Hash, HashError};
pub use identity::{IdentityError, PrivateKey, PublicKey, Signature};
pub use operation::{
    Body, Header, Operation, OperationError, RawOperation, validate_backlink, validate_header,
    validate_operation,
};
#[cfg(feature = "prune")]
pub use prune::PruneFlag;
