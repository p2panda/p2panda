// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_spaces::ActorId;
use thiserror::Error;

use crate::spaces::Group;
use crate::spaces::types::{InnerMember, SpacesManagerError};

// TODO: Needs to provide safety methods to authenticate this member, check if the verifying key &
// key bundle is authentic + that they belong together.
#[derive(Debug)]
pub struct Member {
    pub(crate) inner: InnerMember,
}

impl Member {
    pub fn id(&self) -> ActorId {
        self.inner.id()
    }
}

#[allow(clippy::from_over_into)]
impl Into<ActorId> for Member {
    fn into(self) -> ActorId {
        self.inner.id()
    }
}

impl From<Member> for p2panda_spaces::member::Member {
    fn from(value: Member) -> p2panda_spaces::member::Member {
        value.inner
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupActor {
    pub(crate) id: ActorId,
    pub(crate) group: bool,
}

impl GroupActor {
    pub fn id(&self) -> ActorId {
        self.id
    }

    pub fn is_group(&self) -> bool {
        self.group
    }
}

impl From<p2panda_spaces::GroupActor> for GroupActor {
    fn from(actor: p2panda_spaces::GroupActor) -> Self {
        Self {
            id: actor.id(),
            group: actor.is_group(),
        }
    }
}

impl From<Member> for GroupActor {
    fn from(member: Member) -> Self {
        Self {
            id: member.id(),
            group: false,
        }
    }
}

impl From<Group> for GroupActor {
    fn from(group: Group) -> Self {
        Self {
            id: group.id(),
            group: true,
        }
    }
}

#[allow(clippy::from_over_into)]
impl Into<ActorId> for GroupActor {
    fn into(self) -> ActorId {
        self.id
    }
}

#[derive(Debug, Error)]
pub enum MemberError {
    #[error(transparent)]
    Manager(#[from] SpacesManagerError),
}
