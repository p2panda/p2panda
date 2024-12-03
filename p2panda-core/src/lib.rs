// SPDX-License-Identifier: AGPL-3.0-or-later

//! Core data types used across the p2panda stack to offer distributed, secure and efficient data
//! transfer between peers.
//!
//! The main data type is a highly extensible cryptographically secure append-only log
//! implementation. It provides all the basic features required to implement more advanced
//! distributed data types commonly required when building peer-to-peer and local-first
//! applications.
//!
//! # Features:
//!
//! - cryptographic signatures for authorship verification and tamper-proof messages
//! - various ordering algorithms can be applied over collections of messages
//! - provides mechanisms for efficient sync of past state
//! - is compatible with any networking scenario (even broadcast only)
//!
//! These logs are made up of [`Operation`]s, authors holding a cryptographic key pair sign and
//! append operations them to a log. An author may have one or many logs, and how logs are
//! identified is not further defined in this crate (see extensions).
//!
//! A common challenge in distributed systems is how to order operations written concurrently by
//! different authors and/or processes. Operations contain information which can be used for
//! establishing order depending on ones use case:
//! - `timestamp`: The UNIX timestamp of when the operation was create can be used and will be
//!   suitable for some some environments.
//! - `previous`: An (optional) list of hashes referring to the previous observed operations can
//!   be be used to establish cryptographically secure partial-ordering.
//!
//! It is worth noting that ordering algorithms are _not_ further specified or provided as part of
//! `p2panda-core`.
//!
//! Custom extension fields can be defined by users of this library to introduce additional
//! functionality depending on their particular use cases. p2panda provides our own extensions
//! which are required when using our other crates offering more advanced functionality needed for
//! application building (CRDTs, access control, encryption, ephemeral data, garbage collection,
//! etc...), but it's entirely possible for users to define their own extensions as well.
//!
//! An operation is constructed from a [`Header`] and a [`Body`], the `Header` contains all
//! metadata associated with the particular operation, and the `Body` contains the actual
//! application message bytes.
//!
//! # Examples
//!
//! ```
//! use p2panda_core::{Body, Header, Operation, PrivateKey};
//!
//! // Authors Ed25519 private signing key.
//! let private_key = PrivateKey::new();
//!
//! // Construct the body and header.
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
//!     extensions: None::<()>,
//! };
//!
//! // Sign the header with the authors private key.
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
    validate_backlink, validate_header, validate_operation, Body, Header, Operation,
    OperationError, RawOperation,
};
#[cfg(feature = "prune")]
pub use prune::PruneFlag;
