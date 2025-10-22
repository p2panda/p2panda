// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::NetworkId;
use crate::actors::endpoint::iroh::IrohConfig;

#[derive(Clone, Debug)]
pub struct Config {
    pub network_id: NetworkId,
    pub iroh: IrohConfig,
}

impl Config {
    pub fn from_network_id(network_id: NetworkId) -> Self {
        Self {
            network_id,
            iroh: IrohConfig::default(),
        }
    }
}
