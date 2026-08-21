// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core p2panda data type offering distributed, secure and efficient data transfer between peers.
//!
//! Operations are used to carry any data from one peer to another (distributed), while assuming no
//! reliable network connection (offline-first) and untrusted machines (cryptographically secure).
//! The author of an operation uses it's [`SigningKey`](crate::SigningKey) to cryptographically sign
//! every operation. This can be verified and used for authentication by any other peer.
//!
//! Every operation consists of a [`Header`] and an optional [`Body`]. The body holds arbitrary
//! bytes (up to the application to decide what should be inside). The header is used to
//! cryptographically secure & authenticate the body and for providing ordered collections of
//! operations when required.
//!
//! Operations have a `backlink` and `seq_num` field in the header. These are used to form a linked
//! list of operations, where every subsequent operation points to the previous one by referencing
//! its cryptographically secured hash.
//!
//! Header extensions can be used to add additional information, like "pruning" points for removing
//! old or unwanted data, "tombstones" for explicit deletion, capabilities or group encryption
//! schemes or custom application-related features etc.
//!
//! Operations are encoded in CBOR format and use Ed25519 key pairs for digital signatures and
//! BLAKE3 for hashing.
//!
//! ## Examples
//!
//! ### Construct and sign a header
//!
//! ```
//! use p2panda_core::{Header, SigningKey};
//!
//! let signing_key = SigningKey::generate();
//!
//! let header = Header::builder()
//!     .body(b"Hello, Icebear!")
//!     .build(&signing_key, ());
//! ```
//!
//! ### Custom extensions
//!
//! ```
//! use p2panda_core::{Body, Header, SigningKey, PruneFlag};
//! use serde::{Serialize, Deserialize};
//!
//! let signing_key = SigningKey::generate();
//!
//! #[derive(Clone, Debug, Default, Serialize, Deserialize)]
//! struct CustomExtensions {
//!     prune_flag: PruneFlag,
//! }
//!
//! let extensions = CustomExtensions {
//!     prune_flag: PruneFlag::new(true),
//! };
//!
//! let body = Body::from_bytes("Prune from here please!".as_bytes());
//! let header = Header::builder()
//!     .body(&body)
//!     .build(&signing_key, extensions);
//!
//! assert!(header.extensions.prune_flag.is_set())
//! ```
mod any;
mod body;
mod builder;
mod errors;
mod header;
#[allow(clippy::module_inception)]
mod operation;
#[cfg(test)]
mod tests;
mod validation;

pub use any::{AnyHeader, AnyOperation};
pub use body::Body;
pub use builder::Builder;
pub use errors::{HeaderError, OperationError};
pub use header::{Header, PayloadSize, Version};
pub use operation::{Operation, RawOperation};
pub use validation::{validate_backlink, validate_header, validate_operation};
