// SPDX-License-Identifier: MIT OR Apache-2.0

//! Traits required for defining custom extensions.
//!
//! User-defined extensions can be added to an operation's `Header` in order to extend the basic
//! functionality of the core p2panda data types or to encode application-specific fields which
//! should not be contained in the [`Body`](crate::Body). Extension values can themselves be
//! derived from other header material, such as `PublicKey` or a headers' `Hash`.
//!
//! At a lower level this might be information relating to capabilities or group encryption schemes
//! which is required to enforce access-control restrictions during sync. Alternatively, extensions
//! might be used to set expiration timestamps and deletion flags in order to facilitate garbage
//! collection of stale data from the network. The core p2panda data types intentionally don't
//! enforce a single approach to such areas where there are rightly many different approaches, with
//! the most suitable being dependent on specific use-case requirements.
//!
//! Interfaces which use p2panda core data types can require certain extensions to be present on
//! any headers that their APIs accept using trait bounds. `p2panda-stream`, for example, uses the
//! [`PruneFlag`](crate::PruneFlag) in order to implement automatic network-wide garbage
//! collection.
//!
//! Extensions are encoded on a header and sent over the wire. We need to satisfy all trait
//! requirements that `Header` requires, including `Serialize` and `Deserialize`.
//!
//! //! ## Example
//!
//! ```
//! use p2panda_core::{Body, Hash, Extension, Header, PrivateKey};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Clone, Debug, Serialize, Deserialize)]
//! struct LogId(Hash);
//!
//! #[derive(Clone, Debug, Default, Serialize, Deserialize)]
//! struct Expiry(u64);
//!
//! #[derive(Clone, Debug, Serialize, Deserialize)]
//! struct CustomExtensions {
//!     log_id: Option<LogId>,
//!     expires: Expiry,
//! }
//!
//! impl Extension<LogId> for CustomExtensions {
//!     fn extract(header: &Header<Self>) -> Option<LogId> {
//!         if header.seq_num == 0 {
//!             return Some(LogId(header.hash()));
//!         };
//!
//!         header.extensions.log_id.clone()
//!     }
//! }
//!
//! impl Extension<Expiry> for CustomExtensions {
//!     fn extract(header: &Header<Self>) -> Option<Expiry> {
//!        Some(header.extensions.expires.clone())
//!     }
//! }
//!
//! let extensions = CustomExtensions {
//!     log_id: None,
//!     expires: Expiry(0123456),
//! };
//!
//! let private_key = PrivateKey::new();
//! let body: Body = Body::new("Hello, Sloth!".as_bytes());
//!
//! let mut header = Header {
//!     version: 1,
//!     public_key: private_key.public_key(),
//!     signature: None,
//!     payload_size: body.size(),
//!     payload_hash: Some(body.hash()),
//!     timestamp: 0,
//!     seq_num: 0,
//!     backlink: None,
//!     previous: vec![],
//!     extensions: extensions.clone(),
//! };
//!
//! header.sign(&private_key);
//!
//! let log_id: LogId = header.extension().unwrap();
//! let expiry: Expiry = header.extension().unwrap();
//!
//! assert_eq!(header.hash(), log_id.0);
//! assert_eq!(extensions.expires.0, expiry.0);
//! ```
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::Header;

/// Trait definition of a single header extension type.
pub trait Extension<T>: Extensions {
    /// Extract the extension value from a header.
    fn extract(_header: &Header<Self>) -> Option<T> {
        None
    }
}

/// Super-trait defining trait bounds required by custom extensions types.
pub trait Extensions: Clone + Debug + for<'de> Deserialize<'de> + Serialize {}

/// Blanket implementation of `Extensions` trait any type with the required bounds satisfied.
impl<T> Extensions for T where T: Clone + Debug + for<'de> Deserialize<'de> + Serialize {}
