// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use crate::types::{ActorId, AuthAction, Conditions};

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;

#[derive(Debug)]
pub enum ControlMessage<C> {
    Create {
        // GroupMember is required for understanding if a public key / actor id is an individual or
        // a group in case we're adding something with only pull-access. In that case that actor
        // doesn't need to publish a key bundle and every receiver will not strictly be able to
        // verify if it's _really_ a group or individual.
        //
        // In any other case we always want to verify if the group member type is correct.
        initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    },
    // @TODO: introduce all other variants.
}

impl<C> ControlMessage<C>
where
    C: Conditions,
{
    pub(crate) fn to_auth_action(&self) -> AuthAction<C> {
        match self {
            ControlMessage::Create { initial_members } => AuthAction::Create {
                initial_members: initial_members.to_owned(),
            },
        }
    }
}
