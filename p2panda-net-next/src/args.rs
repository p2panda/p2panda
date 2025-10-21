// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;

use crate::config::Config;

#[derive(Clone, Debug)]
pub struct ApplicationArguments {
    pub config: Config,
    pub private_key: PrivateKey,
}

#[cfg(test)]
impl Default for ApplicationArguments {
    fn default() -> Self {
        Self {
            config: Config::from_network_id([1; 32]),
            private_key: Default::default(),
        }
    }
}
