// SPDX-License-Identifier: AGPL-3.0-or-later

//! Hash type of p2panda using BLAKE23 algorithm wrapped in [`YASMF`] "Yet-Another-Smol-Multi-Format".
//!
//! The original Bamboo specification calls for [`YAMF`] hashes, which
//! have 512 bit Blake2b hashes. We are using YASMF, which has 256 bit
//! Blake3 hashes.
//!
//! [`YASMF`]: https://github.com/bamboo-rs/yasmf-hash
//! [`YAMF`]: https://github.com/bamboo-rs/yamf-hash
mod error;
mod hash;

pub use error::HashError;
pub use hash::{Blake3ArrayVec, Hash};
pub(crate) use hash::HASH_SIZE;
