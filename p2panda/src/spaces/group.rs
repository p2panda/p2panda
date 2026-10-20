// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use p2panda_auth::validation::{
    self, AddMemberError, RemoveMemberError, can_add_member, can_remove_member,
};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::traits::ShortFormat;
use p2panda_spaces::{ActorId, AuthGroupState, GroupId, MemberId, global_groups_id};
use p2panda_store::groups::GroupsStore;
use p2panda_store::{SqliteError, SqliteStore};
use thiserror::Error;

use crate::egress::{EgressError, EgressHandle, SubmitError};
use crate::spaces::repair::{RepairCommandError, RepairTask};
use crate::spaces::types::{
    AuthCapabilities, InnerGroup, InnerGroupError, InnerPendingGroup, NoBody, SpacesManagerError,
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

/// Wraps topic stream and returns the pub/sub pair of a group which might not be created or
/// joined yet.
pub(crate) fn pending_group_stream(
    inner: InnerPendingGroup,
    store: SqliteStore,
    egress_handle: EgressHandle,
    repair_task: RepairTask,
    tx: StreamPublisher<NoBody>,
    rx: StreamSubscription<NoBody>,
) -> (PendingGroup, GroupSubscription) {
    (
        PendingGroup {
            inner,
            store,
            egress_handle,
            repair_task,
            tx,
        },
        GroupSubscription { rx },
    )
}

/// Handle to a subscribed group which might not be created or joined yet.
///
/// Use `created` to wait until the group is known, for example to add it to a space, or `joined` to
/// wait until we've been added to the group.
#[derive(Debug)]
pub struct PendingGroup {
    inner: InnerPendingGroup,
    store: SqliteStore,
    egress_handle: EgressHandle,
    repair_task: RepairTask,
    tx: StreamPublisher<NoBody>,
}

static_assertions::assert_impl_all!(PendingGroup: Send, Sync);

impl PendingGroup {
    pub fn id(&self) -> ActorId {
        self.inner.id()
    }

    /// Wait until the group is known to us.
    ///
    /// Resolves even if we are not a member of the group.
    pub async fn created(self) -> Result<Group, SpacesManagerError> {
        let inner = self.inner.created().await?;
        Ok(Group {
            inner,
            store: self.store,
            egress_handle: self.egress_handle,
            repair_task: self.repair_task,
            tx: self.tx,
        })
    }

    /// Wait until we have joined the group with any access level.
    pub async fn joined(self) -> Result<Group, SpacesManagerError> {
        let inner = self.inner.joined().await?;
        Ok(Group {
            inner,
            store: self.store,
            egress_handle: self.egress_handle,
            repair_task: self.repair_task,
            tx: self.tx,
        })
    }

    /// Gracefully close the group and any associated sync sessions.
    pub async fn close(self) -> Result<(), CloseError> {
        self.tx.close().await
    }
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

        self.repair(Some(actor)).await?;

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

        self.repair(None).await?;

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
    pub async fn members(&self) -> Result<Vec<(MemberId, AccessLevel)>, InnerGroupError> {
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

    async fn repair(&self, new_member: Option<ActorId>) -> Result<(), RepairError> {
        let group_id = self.id();

        // Repair the group to account for any unprocessed changes which may have occurred due to
        // concurrent actions or repair errors. If the new actor is a group then additionally
        // request that the group is incorporated into the group.
        if let Some(actor) = new_member
            && is_group(&self.store, actor).await?
        {
            self.repair_task
                .repair_group_with(group_id, &[actor])
                .await?;
        } else {
            self.repair_task.repair_group(group_id).await?;
        }

        Ok(())
    }
}

pub(crate) async fn is_group(store: &SqliteStore, actor: ActorId) -> Result<bool, SqliteError> {
    let groups_y: AuthGroupState<AuthCapabilities> = store
        .get_groups_state(global_groups_id())
        .await?
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
    Repair(#[from] RepairError),

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
    Repair(#[from] RepairError),

    #[error(transparent)]
    Group(#[from] InnerGroupError),

    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Egress(#[from] EgressError),
}

#[derive(Debug, Error)]
pub enum RepairError {
    #[error(transparent)]
    Repair(#[from] RepairCommandError),

    #[error(transparent)]
    Sqlite(#[from] SqliteError),
}
