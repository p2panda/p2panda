// SPDX-License-Identifier: AGPL-3.0-or-later

//! Traits required for defining custom extensions.
//!
//! User-defined extensions can be added to an operation's `Header` in order to extend the basic
//! functionality of the core p2panda data types or to encode application-specific fields which
//! should not be contained in the [`Body`](crate::Body).
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
//! ## Example
//!
//! ```
//! use p2panda_core::{Body, Extension, Header, Operation, PrivateKey, PruneFlag};
//! use serde::{Serialize, Deserialize};
//!
//! // Extend our operations with an "expiry" field we can use to implement "ephemeral messages" in
//! // our application, which get automatically deleted after the expiration timestamp is due.
//! #[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
//! pub struct Expiry(u64);
//!
//! // Multiple extensions can be combined in a custom type.
//! #[derive(Clone, Debug, Default, Serialize, Deserialize)]
//! struct CustomExtensions {
//!     expiry: Expiry,
//! }
//!
//! // Implement `Extension<T>` for each extension we want to add to our `CustomExtensions`.
//! impl Extension<Expiry> for CustomExtensions {
//!     fn extract(&self) -> Option<Expiry> {
//!         Some(self.expiry.to_owned())
//!     }
//! }
//!
//! // Create a custom extension instance, this can be added to an operation's header.
//! let extensions = CustomExtensions {
//!     expiry: Expiry(1733170246),
//! };
//!
//! // Extract the extension we are interested in.
//! let expiry: Expiry = extensions.extract().expect("expiry field should be set");
//! ```
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::Header;

/// Trait definition of a single header extension type.
pub trait Extension<T>: Extensions {
    /// Extract the raw extension value of an extension based on it's type.
    fn extract(&self) -> Option<T> {
        None
    }

    /// Extract the extension value with the option to derive it from material contained in the
    /// passed header.
    fn with_header(header: &Header<Self>) -> Option<T> {
        match &header.extensions {
            Some(extensions) => extensions.extract(),
            None => None,
        }
    }
}

/// Super-trait defining trait bounds required by custom extensions types.
pub trait Extensions: Clone + Debug + for<'de> Deserialize<'de> + Serialize {}

/// Blanket implementation of `Extensions` trait any type with the required bounds satisfied.
impl<T> Extensions for T where T: Clone + Debug + for<'de> Deserialize<'de> + Serialize {}

/// Generic implementation of `Extension<T>` for `Header<E>` allowing access to the extension
/// values.
impl<T, E> Extension<T> for Header<E>
where
    E: Extension<T>,
{
    fn extract(&self) -> Option<T> {
        <E as Extension<T>>::with_header(&self)
    }
}
