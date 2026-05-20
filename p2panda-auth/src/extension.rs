// SPDX-License-Identifier: MIT OR Apache-2.0

//! Extension arguments required for constructing groups `GroupsOperation`.
use p2panda_core::{Hash, VerifyingKey};
#[cfg(any(test, feature = "serde"))]
use serde::{Deserialize, Serialize};

use crate::group::GroupAction;

/// Additional arguments which can be attached to a p2panda operation in their extensions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "serde"), derive(Deserialize, Serialize))]
pub struct GroupsExtensionArgs<C = ()> {
    pub group_id: VerifyingKey,
    pub action: GroupAction<VerifyingKey, C>,
    pub dependencies: Vec<Hash>,
}
