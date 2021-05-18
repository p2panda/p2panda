//! Hash type of p2panda using BLAKE2b algorithm wrapped in [`YAMF`] "Yet-Another-Multi-Format"
//! according to the Bamboo specification.
//!
/// [`YAMF`]: https://github.com/bamboo-rs/yamf-hash
mod error;
mod hash;

pub use error::HashError;
pub use hash::{Blake2BArrayVec, Hash};
