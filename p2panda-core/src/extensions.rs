// SPDX-License-Identifier: AGPL-3.0-or-later

//! # Examples
//! ```
//! use p2panda_core::{Body, Extension, Header, Operation, PrivateKey, PruneFlag};
//! use serde::{Serialize, Deserialize};
//!
//! // Authors Ed25519 private signing key.
//! let private_key = PrivateKey::new();
//!
//! #[derive(Clone, Debug, Default, Serialize, Deserialize)]
//! struct CustomExtensions {
//!     prune_flag: PruneFlag,
//! }
//!
//!
//! impl Extension<PruneFlag> for CustomExtensions {
//!     fn extract(&self) -> Option<PruneFlag> {
//!         Some(self.prune_flag.to_owned())
//!     }
//! }
//!
//! let extensions = CustomExtensions {
//!     prune_flag: PruneFlag::new(true),
//! };
//!
//! // Construct the body and header.
//! let body = Body::new("Prune from here please!".as_bytes());
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
//!     extensions: Some(extensions),
//! };
//!
//! // Sign the header with the authors private key.
//! header.sign(&private_key);
//!
//! let prune_flag: PruneFlag = header.extract().unwrap();
//! assert!(prune_flag.is_set())
//! ```
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use crate::Header;

/// Extensions can be used to define custom fields in the operation header.
pub trait Extensions:
    Clone + Debug + Default + for<'de> Deserialize<'de> + Serialize + Send + Sync
{
}

impl<T> Extensions for T where
    T: Clone + Debug + Default + for<'de> Deserialize<'de> + Serialize + Send + Sync
{
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct DefaultExtensions {}

impl<T> Extension<T> for DefaultExtensions {
    fn extract(&self) -> Option<T> {
        None
    }
}

pub trait Extension<T>: Extensions {
    fn extract(&self) -> Option<T> {
        None
    }
}

impl<T, E> Extension<T> for Header<E>
where
    E: Extension<T>,
{
    fn extract(&self) -> Option<T> {
        match &self.extensions {
            Some(extensions) => Extension::<T>::extract(extensions),
            None => None,
        }
    }
}
