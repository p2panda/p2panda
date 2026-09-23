// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{FutureExt, Stream, StreamExt};
use p2panda_auth::validation::{
    AddMemberError, DemoteMemberError, PromoteMemberError, RemoveMemberError, WriteError,
    can_add_member, can_demote_member, can_promote_member, can_remove_member, can_write,
};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::Hash;
use p2panda_core::cbor::{EncodeError, encode_cbor};
use p2panda_core::traits::ShortFormat;
use p2panda_net::connection_authoriser::ConnectionAuthoriser;
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::space::SpacesState;
use p2panda_spaces::{ActorId, AuthGroupState, MemberId, SpaceId, SpacesStoreState};
use p2panda_store::groups::GroupsStore;
use p2panda_store::spaces::{SpacesStore, SqliteSpacesStore};
use p2panda_store::{SqliteError, SqliteStore, tx};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::oneshot::error::RecvError;
use tracing::error;

use crate::egress::{EgressError, EgressHandle, SubmitError, SubmitFuture};
use crate::operation::Extensions;
use crate::spaces::authoriser::update_authoriser;
use crate::spaces::member::associate_members;
use crate::spaces::message::SpacesMessage;
use crate::spaces::types::{
    AuthCapabilities, InnerSpace, InnerSpaceError, SpacesEvent, SpacesManagerError,
};
use crate::spaces::{KeyBundleTaskCommand, KeyBundleTaskSender, RepairError, RepairTask};
use crate::streams::{CloseError, StreamEvent, StreamPublisher, StreamSubscription};

/// Wraps topic stream and returns the pub/sub pair of a more specialised spaces stream.
pub(crate) fn spaces_stream<M>(
    inner: InnerSpace,
    store: SqliteStore,
    repair_task: RepairTask,
    key_bundle_task_tx: KeyBundleTaskSender,
    egress_handle: EgressHandle,
    // TODO: Not used, can it be removed now?
    _tx: StreamPublisher<M>,
    rx: StreamSubscription<M>,
    // TODO: Only required until https://github.com/p2panda/p2panda/issues/1362 is resolved.
    connection_authoriser: ConnectionAuthoriser,
) -> (Space<M>, SpaceSubscription<M>)
where
    M: Serialize,
{
    if let Err(err) = key_bundle_task_tx.send(KeyBundleTaskCommand::AddStream(
        inner.id(),
        egress_handle.clone(),
    )) {
        error!(space_id = %inner.id(), "failed adding stream to key bundle task: {err}");
    }

    (
        Space {
            inner,
            store,
            repair_task,
            key_bundle_task_tx,
            egress_handle,
            _tx,
            connection_authoriser,
        },
        SpaceSubscription { rx },
    )
}

#[derive(Debug)]
pub struct Space<M>
where
    M: Serialize,
{
    inner: InnerSpace,
    store: SqliteStore,
    repair_task: RepairTask,
    key_bundle_task_tx: KeyBundleTaskSender,
    egress_handle: EgressHandle,
    _tx: StreamPublisher<M>,
    connection_authoriser: ConnectionAuthoriser,
}

impl<M> Drop for Space<M>
where
    M: Serialize,
{
    fn drop(&mut self) {
        let _ = self
            .key_bundle_task_tx
            .send(KeyBundleTaskCommand::RemoveStream(self.id()));
    }
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
        let members = self.actors().await?;

        can_write(self.inner.me(), &members).map_err(|err| PublishSpaceError::Validation {
            space_id: self.id(),
            err,
        })?;

        // Before publishing messages we trigger and await return from a space repair which will
        // ensure we have incorporated the latest groups changes into the space.
        self.repair().await?;

        // TODO: We'll remove custom `M` types in the future, users will only provide bytes on this
        // level.
        let body_bytes = encode_cbor(&message)?;

        // TODO: This should _not_ encrypt the message (yet), instead the processor will deal with
        // it. The only thing we want from here is a marker that this will be encrypted.
        //
        // We could also handle this outside of p2panda-spaces, simply by coming up with an argument
        // in the extensions for the spaces processor in p2panda-stream.
        let (_, message, _) = self.inner.publish(&body_bytes).await?;

        // We don't need to persist state or pass enriched events through the pipeline as the
        // spaces processor can re-process this event.
        let submit_fut = self.egress_handle.submit(message.into_operation()).await?;

        Ok(SpaceFuture {
            submit_fut,
            space_id: self.inner.id(),
        })
    }

    pub async fn add(
        &self,
        actor: impl Into<ActorId>,
        access: AccessLevel,
    ) -> Result<(), AddSpaceMemberError> {
        let me = self.inner.me();
        let actor = actor.into();
        let members = self.actors().await?;

        can_add_member(me, actor, &members).map_err(|err| AddSpaceMemberError::Validation {
            actor,
            space_id: self.id(),
            err,
        })?;

        // Before performing any action we trigger and await return from a space repair which will
        // ensure we have incorporated the latest groups changes into the space.
        self.repair().await?;

        let (groups_y, space_y, group_message, space_message, events) = self
            .inner
            .add(
                actor,
                Access {
                    conditions: None,
                    level: access,
                },
            )
            .await?;

        // TODO: Only required until https://github.com/p2panda/p2panda/issues/1362 is resolved.
        update_authoriser(&self.connection_authoriser, &events).await;

        self.process_change(groups_y, space_y, [group_message, space_message], events)
            .await?;

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

        // Before performing any action we trigger and await return from a space repair which will
        // ensure we have incorporated the latest groups changes into the space.
        self.repair().await?;

        let (groups_y, space_y, group_message, space_message, events) =
            self.inner.remove(actor).await?;

        // TODO: Only required until https://github.com/p2panda/p2panda/issues/1362 is resolved.
        update_authoriser(&self.connection_authoriser, &events).await;

        self.process_change(groups_y, space_y, [group_message, space_message], events)
            .await?;

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

        // Before performing any action we trigger and await return from a space repair which will
        // ensure we have incorporated the latest groups changes into the space.
        self.repair().await?;

        let (groups_y, space_y, group_message, space_message, events) = self
            .inner
            .promote(
                actor,
                Access {
                    conditions: None,
                    level: access,
                },
            )
            .await?;

        self.process_change(groups_y, space_y, [group_message, space_message], events)
            .await?;

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

        // Before performing any action we trigger and await return from a space repair which will
        // ensure we have incorporated the latest groups changes into the space.
        self.repair().await?;

        let (groups_y, space_y, group_message, space_message, events) = self
            .inner
            .demote(
                actor,
                Access {
                    conditions: None,
                    level: access,
                },
            )
            .await?;

        self.process_change(groups_y, space_y, [group_message, space_message], events)
            .await?;

        Ok(())
    }

    async fn process_change(
        &self,
        groups_y: AuthGroupState<AuthCapabilities>,
        space_y: SpacesState<AuthCapabilities>,
        messages: [SpacesMessage; 2],
        events: Vec<p2panda_spaces::Event<AuthCapabilities>>,
    ) -> Result<(), ProcessError> {
        // Associate member logs with this space. This is equivalent to the member association hook
        // which is registered on the event processing pipeline. Since locally issued events are not
        // going through the pipeline, we have to repeat it here as well.
        associate_members(self.inner.me(), &self.store, &events).await;

        let spaces_store = SqliteSpacesStore::<Extensions>::new(self.store.clone());

        tx!(spaces_store, {
            // Persist the computed groups and spaces state to the stores.
            spaces_store
                .set_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID), &groups_y)
                .await?;
            spaces_store
                .set_space_state_tx(&self.id(), &SpacesStoreState::from(space_y))
                .await?;
        });

        submit_enriched(&self.egress_handle, messages.to_vec(), events).await?;
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

    /// Gracefully close the space and any associated sync sessions.
    pub async fn close(self) -> Result<(), CloseError> {
        self._tx.close().await
    }

    /// Incorporate missing groups messages into the space, any resulting operations are published
    /// live into the space topic.
    pub(crate) async fn repair(&self) -> Result<bool, RepairError> {
        let repaired = self.repair_task.repair().await?;
        Ok(repaired)
    }
}

pub async fn submit_enriched(
    egress_handle: &EgressHandle,
    mut messages: Vec<SpacesMessage>,
    events: Vec<SpacesEvent>,
) -> Result<(), SpaceEgressError> {
    // Pop off the last spaces message, we will attach all events to this one, refactor after https://github.com/p2panda/p2panda/issues/1432
    let Some(last) = messages.pop() else {
        return Ok(());
    };

    // Send all other spaces operations to egress.
    for message in messages {
        let submit_fut = egress_handle.submit(message.into_operation()).await?;
        submit_fut.await?;
    }

    // Send the final spaces operation and events to egress.
    let submit_fut = egress_handle
        .submit_enriched(last.into_operation(), events)
        .await?;
    submit_fut.await?;
    Ok(())
}

pub struct SpaceSubscription<M> {
    #[allow(unused)]
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
    pub(crate) space_id: SpaceId,
    pub(crate) submit_fut: SubmitFuture,
}

impl SpaceFuture {
    pub fn id(&self) -> SpaceId {
        self.space_id
    }
}

impl Future for SpaceFuture {
    type Output = Result<(), EgressError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.submit_fut.poll_unpin(cx)
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
    RepairSpace(#[from] RepairError),

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
    RepairSpace(#[from] RepairError),

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
    RepairSpace(#[from] RepairError),

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
    RepairSpace(#[from] RepairError),

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
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

    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error("couldn't process spaces change due to broken channel")]
    Recv(#[from] RecvError),

    #[error("couldn't send event due to broken app channel")]
    AppSend,
}

#[derive(Debug, Error)]
pub enum PublishSpaceError {
    #[error(
        "failed validation to space {space_id}: {err}",
        space_id = space_id.fmt_short()
    )]
    Validation { space_id: SpaceId, err: WriteError },

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    RepairSpace(#[from] RepairError),
}
