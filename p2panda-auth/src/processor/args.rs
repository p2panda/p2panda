// SPDX-License-Identifier: MIT OR Apache-2.0

//! Arguments required for constructing groups `GroupsOperation` and conversion trait from p2panda
//! operations which contain an `E` extension which implements `Extension<GroupsArgs>`.
use p2panda_core::{Extension, Extensions, Hash, Operation, PublicKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::group::GroupAction;
use crate::processor::operation::GroupsOperation;
use crate::traits::Conditions;

/// Additional operation arguments required for constructing a groups GroupsOperation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GroupsArgs<C = ()> {
    pub group_id: PublicKey,
    pub action: GroupAction<PublicKey, C>,
}

impl<E, C> TryInto<GroupsOperation<PublicKey, Hash, C>> for Operation<E>
where
    E: Extensions + Extension<GroupsArgs<C>>,
    C: Conditions,
{
    type Error = GroupsArgsError;

    fn try_into(self) -> Result<GroupsOperation<PublicKey, Hash, C>, Self::Error> {
        let args = self.header.extension().ok_or(GroupsArgsError {})?;
        Ok(GroupsOperation {
            id: self.hash,
            author: self.header.public_key,
            dependencies: self.header.previous,
            group_id: args.group_id,
            action: args.action,
        })
    }
}

#[derive(Debug, Error)]
#[error("groups args missing from operation extensions")]
pub struct GroupsArgsError;
