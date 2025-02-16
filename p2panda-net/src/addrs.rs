// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;
use std::net::SocketAddr;
use std::str::FromStr;

use anyhow::Context;
use iroh::RelayUrl as IrohRelayUrl;
use iroh::{NodeAddr as IrohNodeAddr, NodeId};
use p2panda_core::PublicKey;
use serde::{Deserialize, Serialize};

use crate::to_public_key;

/// Default STUN port used by the relay server.
///
/// The STUN port as defined by [RFC 8489](<https://www.rfc-editor.org/rfc/rfc8489#section-18.6>)
pub const DEFAULT_STUN_PORT: u16 = 3478;

/// URL identifying a relay server.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct RelayUrl(IrohRelayUrl);

impl RelayUrl {
    pub fn port(&self) -> Option<u16> {
        self.0.port()
    }
}

impl FromStr for RelayUrl {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner = IrohRelayUrl::from_str(s).context("invalid URL")?;
        Ok(Self(inner))
    }
}

impl Display for RelayUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_string())
    }
}

impl From<RelayUrl> for IrohRelayUrl {
    fn from(value: RelayUrl) -> Self {
        value.0
    }
}

/// Converts a `iroh` relay url type to the `p2panda-net` implementation.
pub(crate) fn to_relay_url(url: IrohRelayUrl) -> RelayUrl {
    RelayUrl(url)
}

/// Node address including public key, socket address(es) and an optional relay URL.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct NodeAddress {
    pub public_key: PublicKey,
    pub direct_addresses: Vec<SocketAddr>,
    pub relay_url: Option<RelayUrl>,
}

impl NodeAddress {
    pub fn from_public_key(public_key: PublicKey) -> Self {
        Self {
            public_key,
            direct_addresses: Vec::new(),
            relay_url: None,
        }
    }
}

/// Converts an `iroh` node address type to the `p2panda-net` implementation.
pub(crate) fn to_node_addr(addr: IrohNodeAddr) -> NodeAddress {
    NodeAddress {
        public_key: to_public_key(addr.node_id),
        direct_addresses: addr
            .direct_addresses
            .iter()
            .map(|addr| addr.to_owned())
            .collect(),
        relay_url: addr.relay_url.map(to_relay_url),
    }
}

/// Converts a `p2panda-net` node address type to the `iroh` implementation.
pub(crate) fn from_node_addr(addr: NodeAddress) -> IrohNodeAddr {
    let node_id = NodeId::from_bytes(addr.public_key.as_bytes()).expect("invalid public key");
    let mut node_addr =
        IrohNodeAddr::new(node_id).with_direct_addresses(addr.direct_addresses.to_vec());
    if let Some(url) = addr.relay_url {
        node_addr = node_addr.with_relay_url(url.into());
    }
    node_addr
}
