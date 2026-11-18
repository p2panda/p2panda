// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::iroh_mdns;

/// mDNS discovery mode.
///
/// By default this is set to "active" meaning we are actively advertising our address and public
/// key on local-area networks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MdnsDiscoveryMode {
    /// mDNS discovery disabled.
    Disabled,

    /// Advertise our own and listen for others' discovery announcements.
    #[default]
    Active,

    /// Listen for others' discovery announcements but don't advertise our own.
    Passive,
}

impl From<MdnsDiscoveryMode> for iroh_mdns::MdnsDiscoveryMode {
    fn from(value: MdnsDiscoveryMode) -> Self {
        match value {
            MdnsDiscoveryMode::Disabled => unreachable!(),
            MdnsDiscoveryMode::Active => Self::Active,
            MdnsDiscoveryMode::Passive => Self::Passive,
        }
    }
}
