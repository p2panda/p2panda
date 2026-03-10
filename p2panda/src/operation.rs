// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PruneFlag;
use serde::{Deserialize, Serialize};

pub type Header = p2panda_core::Header<Extensions>;

pub type Operation = p2panda_core::Operation<Extensions>;

/// Versioning for internal extensions format.
pub(crate) const VERSION: u64 = 1;

// TODO: Make sure encoding is canonical over map keys (sort it before serializing).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extensions {
    #[serde(
        skip_serializing_if = "PruneFlag::is_not_set",
        default = "PruneFlag::default"
    )]
    pub prune_flag: PruneFlag,
    pub version: u64,
}

impl Default for Extensions {
    fn default() -> Self {
        Self {
            prune_flag: PruneFlag::default(),
            version: VERSION,
        }
    }
}
