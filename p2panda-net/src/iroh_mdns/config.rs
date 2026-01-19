// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Display;

/// mDNS discovery mode.
///
/// By default this is set to "passive" and we are not actively advertising our endpoint
/// address to the local-area network.
///
/// This default is chosen to prioritize privacy and security, choose "active" mode only if you
/// can trust that leaking your address and public key on local-area networks is safe for the
/// users.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MdnsDiscoveryMode {
    Active,
    #[default]
    Passive,
}

impl Display for MdnsDiscoveryMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            MdnsDiscoveryMode::Active => "active",
            MdnsDiscoveryMode::Passive => "passive",
        };
        write!(f, "{value}")
    }
}

impl MdnsDiscoveryMode {
    pub fn is_active(&self) -> bool {
        self == &MdnsDiscoveryMode::Active
    }
}
