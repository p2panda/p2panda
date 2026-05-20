// SPDX-License-Identifier: MIT OR Apache-2.0

//! Arguments required for constructing groups `GroupsOperation`.
use p2panda_core::{Hash, VerifyingKey};
use serde::{Deserialize, Serialize};

use p2panda_auth::group::GroupAction;

/// Additional arguments which can be attached to a p2panda operation in their extensions.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
// TODO(glyph): Consider renaming to `GroupsExtensionArgs` to further differentiate from
// `GroupsProcessorArgs`.
pub struct GroupsArgs<C = ()> {
    pub group_id: VerifyingKey,
    pub action: GroupAction<VerifyingKey, C>,
    pub dependencies: Vec<Hash>,
}
