// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_core::PublicKey;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct StreamName(PublicKey, Option<String>);

impl StreamName {
    pub fn new(public_key: PublicKey, name: Option<&str>) -> Self {
        Self(public_key, name.map(|value| value.to_owned()))
    }
}

#[derive(Clone, Debug, Default, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PruneFlag(bool);

impl PruneFlag {
    pub fn new(flag: bool) -> Self {
        Self(flag)
    }

    pub fn is_set(&self) -> bool {
        self.0
    }
}
