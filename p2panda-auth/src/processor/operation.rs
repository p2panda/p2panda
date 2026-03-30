// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, PublicKey};
use serde::{Deserialize, Serialize};

use crate::group::GroupAction;
use crate::traits::{Conditions, Operation};

/// Concrete groups operation type.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupsOperation<C = ()> {
    pub id: Hash,
    pub author: PublicKey,
    pub dependencies: Vec<Hash>,
    pub group_id: PublicKey,
    pub action: GroupAction<PublicKey, C>,
}

/// Implementation of groups Operation trait.
impl<C> Operation<PublicKey, Hash, C> for GroupsOperation<C>
where
    C: Conditions,
{
    fn id(&self) -> Hash {
        self.id
    }

    fn author(&self) -> PublicKey {
        self.author
    }

    fn dependencies(&self) -> Vec<Hash> {
        self.dependencies.clone()
    }

    fn group_id(&self) -> PublicKey {
        self.group_id
    }

    fn action(&self) -> GroupAction<PublicKey, C> {
        self.action.clone()
    }
}
