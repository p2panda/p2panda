// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::group::GroupAction;
use p2panda_auth::traits::{Conditions, Operation as AuthOperation};
use p2panda_core::{Hash, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthMessage<C> {
    pub(crate) operation_id: Hash,
    pub(crate) author: VerifyingKey,
    pub(crate) dependencies: Vec<Hash>,
    pub(crate) group_id: VerifyingKey,
    pub(crate) action: GroupAction<VerifyingKey, C>,
}

impl<C> AuthOperation<VerifyingKey, Hash, C> for AuthMessage<C>
where
    C: Conditions,
{
    fn id(&self) -> Hash {
        self.operation_id.to_owned()
    }

    fn author(&self) -> VerifyingKey {
        self.author.to_owned()
    }

    fn dependencies(&self) -> Vec<Hash> {
        self.dependencies.to_owned()
    }

    fn group_id(&self) -> VerifyingKey {
        self.group_id.to_owned()
    }

    fn action(&self) -> p2panda_auth::group::GroupAction<VerifyingKey, C> {
        self.action.to_owned()
    }
}
