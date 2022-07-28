// SPDX-License-Identifier: AGPL-3.0-or-later

//! Hash type of p2panda using BLAKE3 algorithm wrapped in [`YASMF`]
//! "Yet-Another-Smol-Multi-Format".
//!
//! The original Bamboo specification calls for [`YAMF`] hashes, which have 512 bit Blake2b hashes.
//! We are using YASMF, which has shorter, 256 bit Blake3 hashes.
//!
//! [`YASMF`]: https://github.com/bamboo-rs/yasmf-hash
//! [`YAMF`]: https://github.com/bamboo-rs/yamf-hash
pub mod error;
#[allow(clippy::module_inception)]
mod hash;

pub use hash::{Blake3ArrayVec, Hash, HASH_SIZE};
