// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::NodeId;

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

/// Returns a displayable string representing the underlying value in a short format, easy to read
/// during debugging and logging.
pub trait ShortFormat {
    fn fmt_short(&self) -> String;
}

impl ShortFormat for NodeId {
    fn fmt_short(&self) -> String {
        self.to_hex()[0..10].to_string()
    }
}

impl ShortFormat for iroh::EndpointId {
    fn fmt_short(&self) -> String {
        self.to_string()[0..10].to_string()
    }
}

impl ShortFormat for [u8; 32] {
    fn fmt_short(&self) -> String {
        hex::encode(&self[0..5]).to_string()
    }
}

impl ShortFormat for Vec<u8> {
    fn fmt_short(&self) -> String {
        hex::encode(&self[0..5]).to_string()
    }
}

impl ShortFormat for Vec<NodeId> {
    fn fmt_short(&self) -> String {
        let list: Vec<String> = self.iter().map(|addr| addr.fmt_short()).collect();
        format!("[{}]", list.join(", "))
    }
}

impl ShortFormat for Vec<iroh::EndpointId> {
    fn fmt_short(&self) -> String {
        let list: Vec<String> = self
            .iter()
            .map(|addr| addr.fmt_short().to_string())
            .collect();
        format!("[{}]", list.join(", "))
    }
}
