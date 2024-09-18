// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_core::PublicKey;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct StreamName(PublicKey, Option<String>);

#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PruneFlag(bool);

impl PruneFlag {
    pub fn is_set(&self) -> bool {
        self.0
    }
}
