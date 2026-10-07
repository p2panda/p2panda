// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use p2panda_auth::validation::{
    self, AddMemberError, RemoveMemberError, can_add_member, can_remove_member,
};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::Hash;
use p2panda_core::traits::ShortFormat;
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::{ActorId, AuthGroupState, GroupId, MemberId};
use p2panda_store::groups::GroupsStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use thiserror::Error;

use crate::egress::{EgressError, EgressHandle, SubmitError};
use crate::spaces::repair::{RepairCommandError, RepairTask};
use crate::spaces::types::{
    AuthCapabilities, InnerGroup, InnerGroupError, NoBody, SpacesManagerError,
};
use crate::streams::{CloseError, StreamEvent, StreamPublisher, StreamSubscription};

/// Wraps topic stream and returns the pub/sub pair of a more specialised group stream.
pub(crate) fn group_stream(
    inner: InnerGroup,
    store: SqliteStore,
    egress_handle: EgressHandle,
    repair_task: RepairTask,
    tx: StreamPublisher<NoBody>,
    rx: StreamSubscription<NoBody>,
) -> (Group, GroupSubscription) {
    (
        Group {
            inner,
            store,
            egress_handle,
            repair_task,
            tx,
        },
        GroupSubscription { rx },
    )
}

#[derive(Debug)]
pub struct Group {
    inner: InnerGroup,
    store: SqliteStore,
    egress_handle: EgressHandle,
    repair_task: RepairTask,
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
        let group_id = self.id();
        let actor = actor.into();
        let members = self.actors().await?;
        can_add_member(me, actor, &members).map_err(|err| AddGroupMemberError::Validation {
            actor,
            group_id,
            err,
        })?;

        if is_group(&self.store, actor).await? {
            self.repair_task.sync_groups(group_id, vec![actor]).await?;
        }

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

    /// Gracefully close the group and any associated sync sessions.
    pub async fn close(self) -> Result<(), CloseError> {
        self.tx.close().await
    }
}

pub(crate) async fn is_group(store: &SqliteStore, actor: ActorId) -> Result<bool, SqliteError> {
    let groups_y: AuthGroupState<AuthCapabilities> = tx!(
        store,
        store
            .get_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID))
            .await?
    )
    .unwrap_or_default();

    Ok(validation::is_group(&groups_y, actor))
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
    Manager(#[from] SpacesManagerError),

    #[error(transparent)]
    Egress(#[from] EgressError),

    #[error(transparent)]
    Repair(#[from] RepairCommandError),

    #[error(transparent)]
    Sqlite(#[from] SqliteError),
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

    #[error(transparent)]
    Sqlite(#[from] SqliteError),

    #[error(transparent)]
    Repair(#[from] RepairCommandError),
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
