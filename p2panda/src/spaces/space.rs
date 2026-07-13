// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{FutureExt, Stream, StreamExt};
use p2panda_auth::validation::{
    AddMemberError, RemoveMemberError, WriteError, can_add_member, can_remove_member, can_write,
};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::Hash;
use p2panda_core::cbor::{EncodeError, encode_cbor};
use p2panda_core::traits::ShortFormat;
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::{ActorId, MemberId, SpaceContext, SpaceId, SpacesStoreState};
use p2panda_store::groups::GroupsStore;
use p2panda_store::spaces::{SpacesStore, SqliteSpacesStore};
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot;
use tokio::sync::oneshot::error::RecvError;

use crate::node::CreateStreamError;
use crate::operation::Extensions;
use crate::spaces::types::{InnerSpace, InnerSpaceError, SpacesManagerError};
use crate::spaces::{RepairError, RepairStrategy};
use crate::streams::{
    ImportError, LocalStreamFuture, ProcessedOperation, Source, StreamEvent, StreamPublisher,
    StreamSubscription,
};

/// Wraps topic stream and returns the pub/sub pair of a more specialised spaces stream.
pub(crate) fn spaces_stream<M>(
    inner: InnerSpace,
    store: SqliteStore,
    tx: StreamPublisher<M>,
    rx: StreamSubscription<M>,
) -> (Space<M>, SpaceSubscription<M>) {
    (
        Space {
            inner,
            store: SqliteSpacesStore::new(store),
            tx,
        },
        SpaceSubscription { rx },
    )
}

// TODO: We need a way to automatically publish key bundles (if configured, it should also be an
// option to _not_ do that to only allow initial key agreement through side channels which is more
// private).
#[derive(Debug)]
pub struct Space<M> {
    inner: InnerSpace,
    store: SqliteSpacesStore<Extensions>,
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
        let members = self.members().await?;
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
        let (_, message, _event) = self.inner.publish(&body_bytes).await?;

        // @TODO: We don't need to persist the spaces state here as it's possible for the spaces
        // processor to handle our own operations. Not doing this has the benefit of allowing
        // application events to be emitted from the spaces processor, rather than having to
        // construct and send them here manually.
        let processed = self
            .tx
            .import_local(futures_util::stream::once(async {
                message.into_operation()
            }))
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
        let actor = actor.into();
        let members = self.members().await?;
        can_add_member(me, actor, &members).map_err(|err| AddSpaceMemberError::Validation {
            actor,
            space_id: self.id(),
            err,
        })?;

        // Before performing any action we trigger and await return from a space repair which will
        // ensure we have incorporated the latest groups changes into the space.
        self.repair().await?;

        let (groups_y, space_y, auth_message, space_message, events) = self
            .inner
            .add(
                actor,
                Access {
                    conditions: None,
                    level: access,
                },
            )
            .await?;

        let permit = self.store.begin().await?;

        // Persist the computed groups and spaces state to the stores.
        self.store
            .set_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID), &groups_y)
            .await?;
        self.store
            .set_space_state_tx(&self.id(), &SpacesStoreState::from(space_y))
            .await?;

        self.store.commit(permit).await?;

        let processed = self
            .tx
            .import_local(futures_util::stream::iter([
                auth_message.into_operation(),
                space_message.into_operation(),
            ]))
            .await?;

        processed.await?;

        // Manually forward the resulting spaces events to the application layer.
        let events = events
            .into_iter()
            .filter_map(|event| match event {
                p2panda_spaces::Event::Space(space_event) => Some(StreamEvent::Space(space_event)),
                _ => None,
            })
            .collect();

        self.tx
            .to_output_tx
            .send(events)
            .await
            .map_err(|_| AddSpaceMemberError::AppSend)?;

        Ok(())
    }

    pub async fn remove(&self, actor: impl Into<ActorId>) -> Result<(), RemoveSpaceMemberError> {
        let me = self.inner.me();
        let actor = actor.into();
        let members = self.members().await?;
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

        let (groups_y, space_y, auth_message, space_message, events) =
            self.inner.remove(actor).await?;

        let permit = self.store.begin().await?;

        // Persist the computed groups and spaces state to the stores.
        self.store
            .set_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID), &groups_y)
            .await?;
        self.store
            .set_space_state_tx(&self.id(), &SpacesStoreState::from(space_y))
            .await?;

        self.store.commit(permit).await?;

        let processed = self
            .tx
            .import_local(futures_util::stream::iter([
                auth_message.into_operation(),
                space_message.into_operation(),
            ]))
            .await?;

        processed.await?;

        // Manually forward the resulting spaces events to the application layer.
        let events = events
            .into_iter()
            .filter_map(|event| match event {
                p2panda_spaces::Event::Space(space_event) => Some(StreamEvent::Space(space_event)),
                _ => None,
            })
            .collect();

        self.tx
            .to_output_tx
            .send(events)
            .await
            .map_err(|_| RemoveSpaceMemberError::AppSend)?;

        Ok(())
    }

    pub async fn members(&self) -> Result<Vec<(MemberId, AccessLevel)>, InnerSpaceError> {
        self.inner.members().await.map(|members| {
            members
                .iter()
                .map(|(actor, access)| (*actor, access.level.clone()))
                .collect()
        })
    }

    /// Incorporate missing groups messages into the space, any resulting operations are
    /// published live into the space topic.
    pub(crate) async fn repair(&self) -> Result<bool, RepairSpaceError> {
        let (tx, rx) = oneshot::channel();

        // @TODO: Currently we default to merging all groups into the space state, once we do this
        // selectively we can specify the groups to be included (root group + added / removed)
        // using the RepairStrategy::Partial variant.
        self.tx.repair_tx.send((RepairStrategy::Global, tx)).await?;
        let repaired = rx.await??;
        Ok(repaired)
    }

    // TODO: "actors" method to return the _non-flattened_ actors in a group. This will help to
    // build multi-device applications.
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

// TODO: Can we remove this type in favour of `SpaceEvent` from p2panda-spaces?
//
// The only difference is that the `SpaceEvent` does not have a `Processed` variant. In the stream
// we use `StreamEvent::Processed` for returning operations which came from spaces _and_ non-spaces
// contexts.
#[derive(Clone, Debug)]
pub enum SpaceEvent<M> {
    Processed {
        operation: Box<ProcessedOperation<M>>,
        source: Source,
    },
    Created {
        initial_members: Vec<ActorId>,
        context: SpaceContext,
    },
    Added {
        added: ActorId,
        context: SpaceContext,
    },
    Removed {
        removed: ActorId,
        context: SpaceContext,
    },
    Ejected,
}

pub struct SpaceFuture {
    pub(crate) space_id: SpaceId,
    pub(crate) processed: LocalStreamFuture,
}

impl SpaceFuture {
    pub fn id(&self) -> SpaceId {
        self.space_id
    }
}

impl Future for SpaceFuture {
    // TODO: Processor result?
    type Output = Result<(), oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.processed.poll_unpin(cx)
    }
}

#[derive(Debug, Error)]
pub enum AddSpaceMemberError {
    #[error("failed validation adding {actor} to space {space_id}: {err}", actor = actor.fmt_short(), space_id = space_id.fmt_short())]
    Validation {
        actor: ActorId,
        space_id: SpaceId,
        err: AddMemberError,
    },

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Import(#[from] ImportError),

    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error("couldn't process spaces change due to broken channel")]
    Recv(#[from] RecvError),

    #[error("couldn't send event due to broken app channel")]
    AppSend,

    #[error(transparent)]
    RepairSpace(#[from] RepairSpaceError),
}

#[derive(Debug, Error)]
pub enum RemoveSpaceMemberError {
    #[error("failed validation removing {actor} to space {space_id}: {err}", actor = actor.fmt_short(), space_id = space_id.fmt_short())]
    Validation {
        actor: ActorId,
        space_id: SpaceId,
        err: RemoveMemberError,
    },

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Import(#[from] ImportError),

    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error("couldn't process spaces change due to broken channel")]
    Recv(#[from] RecvError),

    #[error("couldn't send event due to broken app channel")]
    AppSend,

    #[error(transparent)]
    RepairSpace(#[from] RepairSpaceError),
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum SpaceError {
    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Manager(#[from] SpacesManagerError),

    #[error(transparent)]
    Import(#[from] ImportError),

    #[error(transparent)]
    CreateStream(#[from] CreateStreamError),

    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error("couldn't process spaces change due to broken channel")]
    Recv(#[from] RecvError),

    #[error("couldn't send event due to broken app channel")]
    AppSend,

    #[error(transparent)]
    RepairSpace(#[from] RepairSpaceError),
}

#[derive(Debug, Error)]
pub enum PublishSpaceError {
    #[error("failed validation to space {space_id}: {err}", space_id = space_id.fmt_short())]
    Validation { space_id: SpaceId, err: WriteError },

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Import(#[from] ImportError),

    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    RepairSpace(#[from] RepairSpaceError),
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum RepairSpaceError {
    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Repair(#[from] RepairError),

    #[allow(clippy::type_complexity)]
    #[error("failed to send on space repair task channel")]
    Send(#[from] SendError<(RepairStrategy, oneshot::Sender<Result<bool, RepairError>>)>),

    #[error("couldn't receive reply from repair task due to broken channel")]
    Recv(#[from] RecvError),
}
