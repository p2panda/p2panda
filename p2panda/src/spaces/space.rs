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
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::space::SpaceOutput;
use p2panda_spaces::{ActorId, MemberId, SpaceId, SpacesStoreState};
use p2panda_store::groups::GroupsStore;
use p2panda_store::spaces::{SpacesStore, SqliteSpacesStore};
use p2panda_store::{SqliteError, SqliteStore, tx};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::egress::{EgressError, EgressHandle, SubmitError, SubmitFuture};
use crate::operation::Extensions;
use crate::spaces::is_group;
use crate::spaces::member::associate_members;
use crate::spaces::message::SpacesMessage;
use crate::spaces::repair::{RepairCommandError, RepairTask};
use crate::spaces::types::{
    AuthCapabilities, InnerSpace, InnerSpaceError, SpacesEvent, SpacesManagerError,
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

/// Space API with methods to publish messages and query and update membership.
///
/// A space represents an encrypted group context where all published messages are encrypted
/// towards current group members. Spaces automatically perform key rotation when members are
/// removed to ensure that access controls are strictly enforced based on active membership.
///
/// The `Space` type acts as a handle to publish messages into the space and represents the primary
/// API for managing space membership. It is returned from the [`crate::Node::create_space()`],
/// [`crate::Node::space()`] and [`crate::Node::space_from()`] methods.
///
/// ## Example
///
/// ```no_run
/// use p2panda_core::SigningKey;
/// use p2panda::Topic;
/// use p2panda::spaces::AccessLevel;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
/// struct SecretData {
///     title: String,
///     content: String,
/// }
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
///
/// // Spawn a p2panda node.
/// let panda = p2panda::spawn().await?;
///
/// // Spaces behave like topic-streams, just that they're encrypted towards members.
/// let topic = Topic::random();
///
/// // Create a space with only us inside.
/// let (panda_space, mut panda_rx) = panda.create_space::<SecretData>(topic).await?;
///
/// let penguin = SigningKey::generate().verifying_key();
///
/// // Add penguin as a member of the space with write access.
/// panda_space.add(penguin, AccessLevel::Write).await?;
///
/// let message = SecretData {
///     title: "Yo Penguin".to_string(),
///     content: "I know where all the tasty fish live.".to_string(),
/// };
///
/// // Publish a message into the space (this will be encrypted toward penguin).
/// //
/// // The message will be decrypted by penguin's node before they receive it from their space
/// // subscription handle. In this way, the encryption and decryption processes are automatic; the
/// // message is published in plaintext on one node, undergoes transport as encrypted bytes, and
/// // emerges decrytped from the space subscription handle of all other active space members.
/// let processing = panda_space.publish(message).await?;
///
/// // Wait for the message to be processed by the local pipeline.
/// processing.await;
/// #
/// # Ok(())
/// # }
/// ```
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

    /// Publishes a message into the space.
    ///
    ///  Every message published into a space can be decrypted by it's members.
    #[allow(clippy::result_large_err)]
    pub async fn publish(&self, message: M) -> Result<SpaceFuture, PublishSpaceError> {
        let members = self.actors().await.map_err(Box::new)?;

        can_write(self.inner.me(), &members).map_err(|err| PublishSpaceError::Validation {
            space_id: self.id(),
            err,
        })?;

        // TODO: We'll remove custom `M` types in the future, users will only provide bytes on this
        // level.
        let body_bytes = encode_cbor(&message)?;

        // TODO: This should _not_ encrypt the message (yet), instead the processor will deal with
        // it. The only thing we want from here is a marker that this will be encrypted.
        //
        // We could also handle this outside of p2panda-spaces, simply by coming up with an argument
        // in the extensions for the spaces processor in p2panda-stream.
        let (_, message, _) = self.inner.publish(&body_bytes).await.map_err(Box::new)?;

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

    /// Adds a member to the space with an associated access level.
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

        // If the new actor is a group then push any required group operations into the space and
        // make log associations.
        if is_group(&self.store, actor).await? {
            self.repair_task
                .sync_and_repair_space_with_group(space_id, actor)
                .await?;
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

        self.process_change(output).await?;

        Ok(())
    }

    /// Removes a member from the space.
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

        let output = self.inner.remove(actor).await?;
        self.process_change(output).await?;

        Ok(())
    }

    /// Promotes an existing member to the assigned access level.
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

    /// Demotes an existing member to the assigned access level.
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

    async fn process_change(
        &self,
        space_output: SpaceOutput<AuthCapabilities, SpacesMessage>,
    ) -> Result<(), ProcessError> {
        // Associate member logs with this space. This is equivalent to the member association hook
        // which is registered on the event processing pipeline. Since locally issued events are not
        // going through the pipeline, we have to repeat it here as well.
        associate_members(self.inner.me(), &self.store, space_output.events()).await;

        let spaces_store = SqliteSpacesStore::<Extensions>::new(self.store.clone());

        tx!(spaces_store, {
            // Persist the computed groups and spaces state to the stores.
            spaces_store
                .set_groups_state_tx(
                    Hash::digest(GLOBAL_GROUPS_CONTEXT_ID),
                    &space_output.groups_y,
                )
                .await?;
            spaces_store
                .set_space_state_tx(&self.id(), &SpacesStoreState::from(space_output.space_y))
                .await?;
        });

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

/// Subscription to events arriving from a space.
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

/// Future to await processing completion of a message published to a space.
pub struct SpaceFuture {
    pub space_id: SpaceId,
    pub processed: SubmitFuture,
}

impl SpaceFuture {
    /// Returns the identifier (32-byte BLAKE3 hash) for the associated space.
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

/// Error occurred when dispatching spaces events for delivery and processing.
#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum SpaceEgressError {
    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Egress(#[from] EgressError),
}

/// Error occurred when adding a member to a space.
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
    Sqlite(#[from] SqliteError),

    #[error(transparent)]
    Repair(#[from] RepairCommandError),

    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
}

/// Error occurred when removing a member from a space.
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
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
}

/// Error occurred when promoting a member in a space.
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
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
}

/// Error occurred when demoting a member in a space.
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
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Process(#[from] ProcessError),
}

/// Error occurred when processing a change to the membership of a space.
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
}

/// Error occurred when publishing a message to a space.
#[derive(Debug, Error)]
pub enum PublishSpaceError {
    #[error(
        "failed validation to space {space_id}: {err}",
        space_id = space_id.fmt_short()
    )]
    Validation { space_id: SpaceId, err: WriteError },

    #[error(transparent)]
    Space(#[from] Box<InnerSpaceError>),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Submit(#[from] SubmitError),

    #[error(transparent)]
    Store(#[from] SqliteError),
}
