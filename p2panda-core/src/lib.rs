// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]
#![cfg_attr(docsrs, feature(doc_cfg))]

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
//! - Compatible with any networking scenario (for example packet radio or mesh-networks)
//! - Fork-tolerant
//! - Off-chain handling of payloads, can be deleted independently of log structure
//! - Pruning of outdated messages
//! - Highly extensible with custom features, for example prefix-deletion, ephemeral
//!   "self-destructing" messages, etc.
//!
//! p2panda logs are made up of [`Operation`]s. Authors sign operations using their cryptographic
//! key (Ed25519) and append them to a hash-chain of operations. An author may have one or many
//! logs. The precise means of identifying logs is not defined by this crate (see extensions).
//!
//! An operation is constructed from a [`Header`] and a [`Body`], the `Header` contains all metadata
//! associated with the particular operation, and the `Body` contains the actual application message
//! bytes. This separation allows "off-chain" handling, where the important bits in the headers are
//! transmitted via an prioritised channel and secondary information, such as the body, can be
//! loaded "lazily". Additionally it allows deletion of payloads without breaking the integrity of
//! the append-only log.
//!
//! ## Extensions
//!
//! Custom extension fields can be defined by users of this library to introduce additional
//! functionality depending on their particular use cases. p2panda provides our own extensions which
//! are required when using our other crates offering more advanced functionality needed for
//! application building (CRDTs, access control, encryption, ephemeral data, garbage collection,
//! etc.), but it's entirely possible for users to define their own extensions as well.
//!
//! ## Examples
//!
//! **Create and sign operations**
//!
//! ```
//! use p2panda_core::{Body, Header, SigningKey};
//!
//! // Every operation is cryptographically authenticated by an author by signing it with an
//! // Ed25519 key pair. This method generates a new private key for us which needs to be securely
//! // stored for re-use.
//! let signing_key = SigningKey::generate();
//!
//! // Operations consist of an body (with the actual application data) and a header,
//! // enhancing the data to be used in distributed networks.
//! let body = Body::from_bytes("Hello, Sloth!".as_bytes());
//!
//! let header = Header::builder()
//!     .body(&body)
//!     // Sign the header with the author's private key. From now on it's ready to be sent!
//!     .build(&signing_key, ());
//! ```
//!
//! **Extend operations with custom features**
//!
//! ```rust
//! use p2panda_core::{Header, SigningKey};
//! use serde::{Serialize, Deserialize};
//!
//! // Extend operations with an "expiry" field we can use to implement  "ephemeral messages"
//! // in our application, which get automatically deleted after the expiration timestamp is due.
//! #[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
//! pub struct Expiry(u64);
//!
//! // Multiple extensions can be combined in a custom type.
//! #[derive(Clone, Debug, Default, Serialize, Deserialize)]
//! struct CustomExtensions {
//!     expiry: Expiry,
//! }
//!
//! let signing_key = SigningKey::generate();
//!
//! let header = Header::builder()
//!     .body(b"Hello, Panda!")
//!     .build(&signing_key, CustomExtensions {
//!         expiry: Expiry(1787246716),
//!     });
//! ```
pub mod cbor;
pub mod cursor;
pub mod hash;
pub mod identity;
pub mod logs;
pub mod operation;
pub mod prune;
mod serde;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
pub mod timestamp;
pub mod topic;
pub mod traits;

pub use cursor::Cursor;
pub use hash::{Hash, HashError};
pub use identity::{IdentityError, Signature, SigningKey, VerifyingKey};
pub use logs::{LogId, SeqNum};
pub use operation::{
    AnyHeader, AnyOperation, Body, Header, HeaderError, Operation, OperationError, RawOperation,
    validate_backlink, validate_header, validate_operation,
};
pub use prune::PruneFlag;
pub use timestamp::Timestamp;
pub use topic::Topic;
pub use traits::{Author, Extensions, OperationId};
