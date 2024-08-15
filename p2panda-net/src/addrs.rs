// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::net::SocketAddr;
use std::str::FromStr;

use anyhow::Context;
use iroh_net::relay::RelayUrl as IrohRelayUrl;
use iroh_net::{NodeAddr as IrohNodeAddr, NodeId};
use p2panda_core::PublicKey;
use serde::{Deserialize, Serialize};

/// The default STUN port used by the relay server.
///
/// The STUN port as defined by [RFC 8489](<https://www.rfc-editor.org/rfc/rfc8489#section-18.6>)
pub const DEFAULT_STUN_PORT: u16 = 3478;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub type NodeAddress = (PublicKey, Vec<SocketAddr>, Option<RelayUrl>);

#[allow(dead_code)]
pub fn to_node_addr(
    public_key: PublicKey,
    addresses: Vec<SocketAddr>,
    relay: Option<RelayUrl>,
) -> IrohNodeAddr {
    let node_id = NodeId::from_bytes(public_key.as_bytes()).expect("invalid public key");
    let mut node_addr = IrohNodeAddr::new(node_id).with_direct_addresses(addresses.to_vec());
    if let Some(url) = relay {
        node_addr = node_addr.with_relay_url(url.into());
    }
    node_addr
}
