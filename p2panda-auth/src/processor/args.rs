// SPDX-License-Identifier: MIT OR Apache-2.0

//! Arguments required for constructing groups `GroupsOperation`.
use p2panda_core::{Hash, PublicKey};
use serde::{Deserialize, Serialize};

use crate::group::GroupAction;

/// Additional arguments which can be attached to a p2panda operation in their extensions.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GroupsArgs<C = ()> {
    pub group_id: PublicKey,
    pub action: GroupAction<PublicKey, C>,
    pub dependencies: Vec<Hash>,
}
