// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{FutureExt, Stream, StreamExt};
use p2panda_auth::validation::{
    AddMemberError, DemoteMemberError, PromoteMemberError, RemoveMemberError, WriteError,
    can_add_member, can_demote_member, can_promote_member, can_remove_member, can_write,
};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::cbor::{EncodeError, encode_cbor};
use p2panda_core::traits::ShortFormat;
use p2panda_spaces::space::SpaceOutput;
use p2panda_spaces::{ActorId, MemberId, SpaceId};
use p2panda_store::{SqliteError, SqliteStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::egress::{EgressError, EgressHandle, SubmitError, SubmitFuture};
use crate::spaces::is_group;
use crate::spaces::member::associate_members;
use crate::spaces::message::SpacesMessage;
use crate::spaces::repair::{RepairCommandError, RepairTask};
use crate::spaces::types::{
    AuthCapabilities, InnerPendingSpace, InnerSpace, InnerSpaceError, SpacesEvent,
    SpacesManagerError,
};
use crate::streams::{CloseError, StreamEvent, StreamPublisher, StreamSubscription};

/// Wraps topic stream and returns the pub/sub pair of a more specialised spaces stream.
pub(crate) fn spaces_stream<M>(
    inner: InnerSpace,
    store: SqliteStore,
    egress_handle: EgressHandle,
    repair_task: RepairTask,
    tx: StreamPublisher<M>,
    rx: StreamSubscription<M>,
) -> (Space<M>, SpaceSubscription<M>)
where
    M: Serialize,
{
    (
        Space {
            inner,
            store,
            egress_handle,
            repair_task,
            tx,
        },
        SpaceSubscription { rx },
    )
}

/// Wraps topic stream and returns the pub/sub pair of a space which might not be joined yet.
pub(crate) fn pending_spaces_stream<M>(
    inner: InnerPendingSpace,
    store: SqliteStore,
    egress_handle: EgressHandle,
    repair_task: RepairTask,
    tx: StreamPublisher<M>,
    rx: StreamSubscription<M>,
) -> (PendingSpace<M>, SpaceSubscription<M>)
where
    M: Serialize,
{
    (
        PendingSpace {
            inner,
            store,
            egress_handle,
            repair_task,
            tx,
        },
        SpaceSubscription { rx },
    )
}

/// Handle to a subscribed space which might not be joined yet.
///
/// Use `joined` to wait until we've been added to the space and can read and publish messages.
#[derive(Debug)]
pub struct PendingSpace<M>
where
    M: Serialize,
{
    inner: InnerPendingSpace,
    store: SqliteStore,
    egress_handle: EgressHandle,
    repair_task: RepairTask,
    tx: StreamPublisher<M>,
}

impl<M> PendingSpace<M>
where
    M: Serialize,
{
    pub fn id(&self) -> SpaceId {
        self.inner.id()
    }

    /// Wait until we've joined the space with read access or higher.
    pub async fn joined(self) -> Result<Space<M>, SpacesManagerError> {
        let inner = self.inner.joined().await?;
        Ok(Space {
            inner,
            store: self.store,
            egress_handle: self.egress_handle,
            repair_task: self.repair_task,
            tx: self.tx,
        })
    }

    /// Gracefully close the space and any associated sync sessions.
    pub async fn close(self) -> Result<(), CloseError> {
        self.tx.close().await
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<M> PendingSpace<M>
where
    M: Serialize,
{
    /// Inner topic stream publishing handle (tx).
    pub fn inner_tx(&self) -> &StreamPublisher<M> {
        &self.tx
    }
}

#[derive(Debug)]
pub struct Space<M>
where
    M: Serialize,
{
    inner: InnerSpace,
    store: SqliteStore,
    egress_handle: EgressHandle,
    repair_task: RepairTask,
    tx: StreamPublisher<M>,
}

impl<M> Space<M>
where
    M: Serialize,
{
    pub fn id(&self) -> SpaceId {
        self.inner.id()
    }

    #[allow(clippy::result_large_err)]
    pub async fn publish(&self, message: M) -> Result<SpaceFuture, PublishSpaceError> {
        let members = self.actors().await.map_err(Box::new)?;

        can_write(self.inner.me(), &members).map_err(|err| PublishSpaceError::Validation {
            space_id: self.id(),
            err,
        })?;

        self.repair(None).await.map_err(Box::new)?;

        // TODO: We'll remove custom `M` types in the future, users will only provide bytes on this
        // level.
        let body_bytes = encode_cbor(&message)?;

        // TODO: This should _not_ encrypt the message (yet), instead the processor will deal with
        // it. The only thing we want from here is a marker that this will be encrypted.
        //
        // We could also handle this outside of p2panda-spaces, simply by coming up with an argument
        // in the extensions for the spaces processor in p2panda-stream.
        let (message, _) = self.inner.publish(&body_bytes).await.map_err(Box::new)?;

        // We don't need to persist state or pass enriched events through the pipeline as the spaces
        // processor can re-process this event.
        let processed = self
            .egress_handle
            .dispatch(message.into_operation(), self.id().into())
            .await?;

        Ok(SpaceFuture {
            processed,
            space_id: self.inner.id(),
        })
    }

    pub async fn add(
        &self,
        actor: impl Into<ActorId>,
        access: AccessLevel,
    ) -> Result<(), AddSpaceMemberError> {
        let me = self.inner.me();
        let space_id = self.id();
        let actor = actor.into();
        let members = self.actors().await?;

        can_add_member(me, actor, &members).map_err(|err| AddSpaceMemberError::Validation {
            actor,
            space_id,
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

        self.process_change(output).await?;

        Ok(())
    }

    pub async fn remove(&self, actor: impl Into<ActorId>) -> Result<(), RemoveSpaceMemberError> {
        let me = self.inner.me();
        let actor = actor.into();
        let members = self.actors().await?;

        can_remove_member(me, actor, &members).map_err(|err| {
            RemoveSpaceMemberError::Validation {
                actor,
                space_id: self.id(),
                err,
            }
        })?;

        self.repair(None).await?;

        let output = self.inner.remove(actor).await?;
        self.process_change(output).await?;

        Ok(())
    }

    /// Promote an existing space member to the assigned access level.
    pub async fn promote(
        &self,
        actor: impl Into<ActorId>,
        access: AccessLevel,
    ) -> Result<(), PromoteSpaceMemberError> {
        let me = self.inner.me();
        let actor = actor.into();
        let members = self.actors().await?;

        can_promote_member(me, actor, access, &members).map_err(|err| {
            PromoteSpaceMemberError::Validation {
                actor,
                access,
                space_id: self.id(),
                err,
            }
        })?;

        self.repair(None).await?;

        let output = self
            .inner
            .promote(
                actor,
                Access {
                    conditions: None,
                    level: access,
                },
            )
            .await?;

        self.process_change(output).await?;

        Ok(())
    }

    /// Demote an existing space member to the assigned access level.
    pub async fn demote(
        &self,
        actor: impl Into<ActorId>,
        access: AccessLevel,
    ) -> Result<(), DemoteSpaceMemberError> {
        let me = self.inner.me();
        let actor = actor.into();
        let members = self.actors().await?;

        can_demote_member(me, actor, access, &members).map_err(|err| {
            DemoteSpaceMemberError::Validation {
                actor,
                access,
                space_id: self.id(),
                err,
            }
        })?;

        self.repair(None).await?;

        let output = self
            .inner
            .demote(
                actor,
                Access {
                    conditions: None,
                    level: access,
                },
            )
            .await?;

        self.process_change(output).await?;

        Ok(())
    }

    async fn repair(&self, new_member: Option<ActorId>) -> Result<(), RepairError> {
        let space_id = self.id();

        // Repair the space to account for any unprocessed changes which may have occurred due to
        // concurrent actions or repair errors. If the new actor is a group then additionally
        // request that the group is incorporated into the space.
        if let Some(actor) = new_member
            && is_group(&self.store, actor).await?
        {
            self.repair_task.repair_space_with(space_id, actor).await?;
        } else {
            self.repair_task.repair_space(space_id).await?;
        }

        Ok(())
    }

    async fn process_change(
        &self,
        space_output: SpaceOutput<AuthCapabilities, SpacesMessage>,
    ) -> Result<(), ProcessError> {
        // Associate member logs with this space. This is equivalent to the member association hook
        // which is registered on the event processing pipeline. Since locally issued events are not
        // going through the pipeline, we have to repeat it here as well.
        associate_members(self.inner.me(), &self.store, space_output.events()).await;

        dispatch_spaces_events(&self.egress_handle, self.id(), space_output.messages).await?;

        Ok(())
    }

    /// Returns all space members and their access level.
    ///
    /// These members are all the individuals in the space after nested groups have been
    /// flattened.
    pub async fn members(&self) -> Result<Vec<(MemberId, AccessLevel)>, InnerSpaceError> {
        self.inner.members().await.map(|members| {
            members
                .iter()
                .map(|(actor, access)| (*actor, access.level))
                .collect()
        })
    }

    /// Returns all space actors (groups and individuals) and their access levels.
    pub async fn actors(&self) -> Result<Vec<(MemberId, AccessLevel)>, InnerSpaceError> {
        self.inner.actors().await.map(|actors| {
            actors
                .iter()
                .map(|(actor, access)| (*actor, access.level))
                .collect()
        })
    }

    pub async fn group_id(&self) -> Result<ActorId, InnerSpaceError> {
        let group_id = self.inner.group_id().await?;
        Ok(group_id)
    }

    /// Gracefully close the space and any associated sync sessions.
    pub async fn close(self) -> Result<(), CloseError> {
        self.tx.close().await
    }
}

pub(crate) async fn dispatch_spaces_events(
    egress_handle: &EgressHandle,
    space_id: SpaceId,
    events: Vec<(SpacesMessage, Vec<SpacesEvent>)>,
) -> Result<(), SpaceEgressError> {
    let topic = space_id.into();

    for (message, events) in events {
        let processed = if events.is_empty() {
            egress_handle
                .dispatch(message.into_operation(), topic)
                .await?
        } else {
            egress_handle
                .dispatch_with_spaces_events(message.into_operation(), topic, Some(events))
                .await?
        };

        processed.await?;
    }

    Ok(())
}

#[cfg(any(test, feature = "test_utils"))]
impl<M> Space<M>
where
    M: Serialize,
{
    /// Inner topic stream publishing handle (tx).
    pub fn inner_tx(&self) -> &StreamPublisher<M> {
        &self.tx
    }
}

pub struct SpaceSubscription<M> {
    rx: StreamSubscription<M>,
}

impl<M> Stream for SpaceSubscription<M>
where
    M: std::fmt::Debug + Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    type Item = StreamEvent<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_next_unpin(cx)
    }
}

pub struct SpaceFuture {
    pub space_id: SpaceId,
    pub processed: SubmitFuture,
}

impl SpaceFuture {
    pub fn id(&self) -> SpaceId {
        self.space_id
    }
}

impl Future for SpaceFuture {
    type Output = Result<(), EgressError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.processed.poll_unpin(cx)
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum SpaceEgressError {
    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Egress(#[from] EgressError),
}

#[derive(Debug, Error)]
pub enum AddSpaceMemberError {
    #[error(
        "failed validation adding {actor} to space {space_id}: {err}",
        actor = actor.fmt_short(),
        space_id = space_id.fmt_short()
    )]
    Validation {
        actor: ActorId,
        space_id: SpaceId,
        err: AddMemberError,
    },

    #[error(transparent)]
    Repair(#[from] RepairError),

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
}

#[derive(Debug, Error)]
pub enum RemoveSpaceMemberError {
    #[error(
        "failed validation removing {actor} to space {space_id}: {err}",
        actor = actor.fmt_short(),
        space_id = space_id.fmt_short()
    )]
    Validation {
        actor: ActorId,
        space_id: SpaceId,
        err: RemoveMemberError,
    },

    #[error(transparent)]
    Repair(#[from] RepairError),

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
}

#[derive(Debug, Error)]
pub enum PromoteSpaceMemberError {
    #[error(
        "failed validation promoting {actor} to {access} access in space {space_id}: {err}",
        actor = actor.fmt_short(),
        space_id = space_id.fmt_short()
    )]
    Validation {
        actor: ActorId,
        access: AccessLevel,
        space_id: SpaceId,
        err: PromoteMemberError,
    },

    #[error(transparent)]
    Repair(#[from] RepairError),

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
}

#[derive(Debug, Error)]
pub enum DemoteSpaceMemberError {
    #[error(
        "failed validation demoting {actor} to {access} access in space {space_id}: {err}",
        actor = actor.fmt_short(),
        space_id = space_id.fmt_short()
    )]
    Validation {
        actor: ActorId,
        access: AccessLevel,
        space_id: SpaceId,
        err: DemoteMemberError,
    },

    #[error(transparent)]
    Repair(#[from] RepairError),

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
}

#[derive(Debug, Error)]
pub enum RepairError {
    #[error(transparent)]
    Repair(#[from] RepairCommandError),

    #[error(transparent)]
    Sqlite(#[from] SqliteError),
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum ProcessError {
    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Manager(#[from] SpacesManagerError),

    #[error(transparent)]
    SpaceEgress(#[from] SpaceEgressError),
}

#[derive(Debug, Error)]
pub enum PublishSpaceError {
    #[error(
        "failed validation to space {space_id}: {err}",
        space_id = space_id.fmt_short()
    )]
    Validation { space_id: SpaceId, err: WriteError },

    #[error(transparent)]
    Repair(#[from] Box<RepairError>),

    #[error(transparent)]
    Space(#[from] Box<InnerSpaceError>),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Submit(#[from] SubmitError),
}
