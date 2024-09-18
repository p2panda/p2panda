// SPDX-License-Identifier: AGPL-3.0-or-later

// @TODO: Move extensions to `p2panda-streams`
use p2panda_core::PublicKey;
use serde::{Deserialize, Serialize};

// @TODO: Implement modes when no public key or stream name is set
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
