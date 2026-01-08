// SPDX-License-Identifier: MIT OR Apache-2.0

mod actors;
mod api;
mod builder;
mod config;
mod discovery;
#[cfg(feature = "supervisor")]
mod supervisor;
#[cfg(test)]
mod tests;
pub(crate) mod user_data;

pub use api::{Endpoint, EndpointError};
pub use builder::{Builder, DEFAULT_NETWORK_ID};
pub use config::{DEFAULT_BIND_PORT, IrohConfig};

/// Converts an `iroh` public key type to the `p2panda-core` implementation.
pub fn to_public_key(key: iroh_base::PublicKey) -> p2panda_core::PublicKey {
    p2panda_core::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}

/// Converts a `p2panda-core` public key to the "iroh" type.
pub fn from_public_key(key: p2panda_core::PublicKey) -> iroh_base::PublicKey {
    iroh_base::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}

/// Converts a `p2panda-core` private key to the "iroh" type.
pub fn from_private_key(key: p2panda_core::PrivateKey) -> iroh_base::SecretKey {
    iroh_base::SecretKey::from_bytes(key.as_bytes())
}
