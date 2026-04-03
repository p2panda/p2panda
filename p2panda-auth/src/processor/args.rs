// SPDX-License-Identifier: MIT OR Apache-2.0

//! Arguments required for constructing groups `GroupsOperation` and conversion trait from p2panda
//! operations which contain an `E` extension which implements `Extension<GroupsArgs>`.
use p2panda_core::{Extension, Extensions, Hash, Operation, PublicKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::group::GroupAction;
use crate::processor::operation::GroupsOperation;
use crate::traits::Conditions;

/// Additional arguments which can be attached to a p2panda operation in their extensions.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GroupsArgs<C = ()> {
    pub group_id: PublicKey,
    pub action: GroupAction<PublicKey, C>,
    pub dependencies: Vec<Hash>,
}

/// Conversion trait for getting a GroupsOperation from a generic p2panda Operation. If this
/// conversion fails then the required arguments were not present on the operation extensions.
impl<E, C> TryInto<GroupsOperation<C>> for Operation<E>
where
    E: Extensions + Extension<GroupsArgs<C>>,
    C: Conditions,
{
    type Error = GroupsArgsError;

    fn try_into(self) -> Result<GroupsOperation<C>, Self::Error> {
        let args = self.header.extension().ok_or(GroupsArgsError {})?;
        Ok(GroupsOperation {
            id: self.hash,
            author: self.header.public_key,
            dependencies: args.dependencies,
            group_id: args.group_id,
            action: args.action,
        })
    }
}

#[derive(Debug, Error)]
#[error("missing \"groups\" operation extensions")]
pub struct GroupsArgsError;
