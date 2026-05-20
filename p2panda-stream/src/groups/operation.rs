// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, VerifyingKey};
use serde::{Deserialize, Serialize};

use p2panda_auth::group::GroupAction;
use p2panda_auth::traits::{Conditions, Operation};

/// Concrete groups operation type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupsOperation<C = ()> {
    pub id: Hash,
    pub author: VerifyingKey,
    pub dependencies: Vec<Hash>,
    pub group_id: VerifyingKey,
    pub action: GroupAction<VerifyingKey, C>,
}

/// Implementation of groups Operation trait.
impl<C> Operation<VerifyingKey, Hash, C> for GroupsOperation<C>
where
    C: Conditions,
{
    fn id(&self) -> Hash {
        self.id
    }

    fn author(&self) -> VerifyingKey {
        self.author
    }

    fn dependencies(&self) -> Vec<Hash> {
        self.dependencies.clone()
    }

    fn group_id(&self) -> VerifyingKey {
        self.group_id
    }

    fn action(&self) -> GroupAction<VerifyingKey, C> {
        self.action.clone()
    }
}
