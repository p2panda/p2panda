// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use p2panda_auth::validation::{
    AddMemberError, RemoveMemberError, can_add_member, can_remove_member,
};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::traits::ShortFormat;
use p2panda_spaces::{ActorId, GroupId, MemberId};
use thiserror::Error;

use crate::egress::{EgressError, EgressHandle, SubmitError};
use crate::node::CreateStreamError;
use crate::processor::ProcessorError;
use crate::spaces::types::{InnerGroup, InnerGroupError, NoBody, SpacesManagerError};
use crate::streams::{StreamEvent, StreamPublisher, StreamSubscription};

/// Wraps topic stream and returns the pub/sub pair of a more specialised group stream.
pub(crate) fn group_stream(
    inner: InnerGroup,
    egress_handle: EgressHandle,
    tx: StreamPublisher<NoBody>,
    rx: StreamSubscription<NoBody>,
) -> (Group, GroupSubscription) {
    (
        Group {
            inner,
            egress_handle,
            tx,
        },
        GroupSubscription { rx },
    )
}

#[derive(Debug)]
pub struct Group {
    inner: InnerGroup,
    egress_handle: EgressHandle,
    #[allow(unused)]
    tx: StreamPublisher<NoBody>,
}

static_assertions::assert_impl_all!(Group: Send, Sync);

impl Group {
    pub fn id(&self) -> ActorId {
        self.inner.id()
    }

    /// Add a new member to the group.
    pub async fn add(
        &self,
        actor: impl Into<ActorId>,
        access: AccessLevel,
    ) -> Result<(), AddGroupMemberError> {
        let me = self.inner.my_id();
        let actor = actor.into();
        let members = self.actors().await?;
        can_add_member(me, actor, &members).map_err(|err| AddGroupMemberError::Validation {
            actor,
            group_id: self.id(),
            err,
        })?;

        let output = self
            .inner
            .add(
                actor,
                Access {
                    conditions: None,
                    level: access,
                },
            )
            .await?;

        // TODO: we might want to persist state and dispatch enriched event.
        let processed = self
            .egress_handle
            .dispatch(output.message.into_operation(), self.id().into())
            .await?;
        processed.await?;

        Ok(())
    }

    /// Remove an existing member from the group.
    pub async fn remove(&self, actor: impl Into<ActorId>) -> Result<(), RemoveGroupMemberError> {
        let me = self.inner.my_id();
        let actor = actor.into();
        let members = self.actors().await?;
        can_remove_member(me, actor, &members).map_err(|err| {
            RemoveGroupMemberError::Validation {
                actor,
                group_id: self.id(),
                err,
            }
        })?;

        let output = self.inner.remove(actor).await?;

        // TODO: we might want to persist state and dispatch enriched event.
        let processed = self
            .egress_handle
            .dispatch(output.message.into_operation(), self.id().into())
            .await?;
        processed.await?;

        Ok(())
    }

    /// Returns all group members and their access level.
    ///
    /// These members are all the individuals in the group after nested groups have been
    /// flattened.
    pub async fn members(&self) -> Result<Vec<(MemberId, AccessLevel)>, GroupError> {
        let result = self.inner.members().await.map(|members| {
            members
                .iter()
                .map(|(actor, access)| (*actor, access.level))
                .collect()
        })?;

        Ok(result)
    }

    /// Returns all group actors (groups and individuals) and their access levels.
    pub async fn actors(&self) -> Result<Vec<(MemberId, AccessLevel)>, InnerGroupError> {
        self.inner.actors().await.map(|actors| {
            actors
                .iter()
                .map(|(actor, access)| (*actor, access.level))
                .collect()
        })
    }
}

impl From<Group> for ActorId {
    fn from(value: Group) -> Self {
        value.inner.id()
    }
}

pub struct GroupSubscription {
    rx: StreamSubscription<NoBody>,
}

impl Stream for GroupSubscription {
    type Item = StreamEvent<NoBody>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_next_unpin(cx)
    }
}

#[derive(Debug, Error)]
pub enum GroupError {
    #[error(transparent)]
    Group(#[from] InnerGroupError),

    #[error(transparent)]
    Processor(#[from] ProcessorError),

    #[error(transparent)]
    Manager(#[from] SpacesManagerError),

    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Egress(#[from] EgressError),

    #[error(transparent)]
    CreateStream(#[from] CreateStreamError),
}

#[derive(Debug, Error)]
pub enum AddGroupMemberError {
    #[error(
        "failed validation adding {actor} to group {group_id}: {err}",
        actor = actor.fmt_short(),
        group_id = group_id.fmt_short()
    )]
    Validation {
        actor: ActorId,
        group_id: GroupId,
        err: AddMemberError,
    },

    #[error(transparent)]
    Group(#[from] InnerGroupError),

    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Egress(#[from] EgressError),
}

#[derive(Debug, Error)]
pub enum RemoveGroupMemberError {
    #[error(
        "failed validation removing {actor} to group {group_id}: {err}",
        actor = actor.fmt_short(),
        group_id = group_id.fmt_short()
    )]
    Validation {
        actor: ActorId,
        group_id: GroupId,
        err: RemoveMemberError,
    },

    #[error(transparent)]
    Group(#[from] InnerGroupError),

    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Egress(#[from] EgressError),
}
