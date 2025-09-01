mod actors;
mod addrs;
mod network;

pub type TopicId = [u8; 32];

/// Converts an `iroh` public key type to the `p2panda-core` implementation.
pub(crate) fn to_public_key(key: iroh_base::PublicKey) -> p2panda_core::PublicKey {
    p2panda_core::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}

/// Converts a `p2panda-core` public key to the "iroh" type.
pub(crate) fn from_public_key(key: p2panda_core::PublicKey) -> iroh_base::PublicKey {
    iroh_base::PublicKey::from_bytes(key.as_bytes()).expect("already validated public key")
}

/// Converts a `p2panda-core` private key to the "iroh" type.
pub(crate) fn from_private_key(key: p2panda_core::PrivateKey) -> iroh_base::SecretKey {
    iroh_base::SecretKey::from_bytes(key.as_bytes())
}
