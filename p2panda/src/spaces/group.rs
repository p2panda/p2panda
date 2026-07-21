// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::sync::RwLock;
use std::task::{Context, Poll};

use futures_util::{FutureExt, Stream};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::VerifyingKey;
use p2panda_spaces::{ActorId, GroupContext, GroupId, MemberId};
use thiserror::Error;
use tokio::sync::{broadcast, oneshot};
use tokio::task::AbortHandle;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::node::CreateStreamError;
use crate::processor::ProcessorError;
use crate::spaces::GroupActor;
use crate::spaces::types::{
    AuthCapabilities, InnerGroup, InnerGroupError, InnerGroupEvent, NoBody, SpacesManagerError,
};
use crate::streams::{
    ImportError, LocalStreamFuture, StreamPublisher, StreamSubscription, SystemEvent,
};

#[derive(Debug)]
pub struct Group {
    inner: InnerGroup,
    tx: StreamPublisher<NoBody>,
    #[allow(unused)]
    rx: StreamSubscription<NoBody>,
    event_stream_rx: RwLock<broadcast::Receiver<GroupEvent>>,
    event_stream_handle: AbortHandle,
}

static_assertions::assert_impl_all!(Group: Send, Sync);

impl Drop for Group {
    fn drop(&mut self) {
        self.event_stream_handle.abort();
    }
}

impl Group {
    pub(crate) fn new(
        inner: InnerGroup,
        tx: StreamPublisher<NoBody>,
        rx: StreamSubscription<NoBody>,
        mut in_event_stream_rx: broadcast::Receiver<SystemEvent>,
    ) -> Self {
        let (out_event_stream_tx, out_event_stream_rx) = broadcast::channel::<GroupEvent>(256);

        let group_id = inner.id();
        let event_stream_handle = tokio::spawn(async move {
            while let Ok(event) = in_event_stream_rx.recv().await {
                let SystemEvent::Groups {
                    inner: group_event, ..
                } = event
                else {
                    continue;
                };

                // If this group is not effected by the action which triggered this event then
                // don't forward it. This includes checking if the group is a parent effected by a
                // childs membership change. We actually want to forward events for all children
                // groups so here we only filter out events which are completely unrelated to the
                // current group.
                if !group_event.effected_group(group_id) && group_id != group_event.group_id() {
                    continue;
                }

                let _ = out_event_stream_tx.send(to_group_event(group_id, group_event));
            }
        });

        Self {
            inner,
            tx,
            rx,
            event_stream_rx: RwLock::new(out_event_stream_rx),
            event_stream_handle: event_stream_handle.abort_handle(),
        }
    }

    pub fn id(&self) -> ActorId {
        self.inner.id()
    }

    pub fn event_stream(&self) -> impl Stream<Item = GroupEvent> + Send + Unpin + 'static {
        // Make sure we're not re-subscribing and thus dropping all events which might be still in
        // the buffer of the first broadcast receiver instance.
        let stream = {
            let write = self.event_stream_rx.write().unwrap();
            let resubscribed = write.resubscribe();

            let mut write = write;
            std::mem::replace(&mut *write, resubscribed)
        };

        // TODO: Check if we really want to silence broadcast "lagged" errors here?
        let stream = BroadcastStream::new(stream).filter_map(|event| event.ok());

        Box::pin(stream)
    }

    pub async fn add(
        &self,
        actor: impl Into<ActorId>,
        access: AccessLevel,
    ) -> Result<GroupFuture, GroupError> {
        let (_, message, _events) = self
            .inner
            .add(
                actor.into(),
                Access {
                    conditions: None,
                    level: access,
                },
            )
            .await?;

        let processed = self
            .tx
            .import_local(futures_util::stream::once(async {
                message.into_operation()
            }))
            .await?;

        Ok(GroupFuture {
            processed,
            group_id: self.inner.id(),
        })
    }

    pub async fn remove(&self, actor: impl Into<ActorId>) -> Result<GroupFuture, GroupError> {
        let (_, message, _events) = self.inner.remove(actor.into()).await?;

        let processed = self
            .tx
            .import_local(futures_util::stream::once(async {
                message.into_operation()
            }))
            .await?;

        Ok(GroupFuture {
            processed,
            group_id: self.inner.id(),
        })
    }

    pub async fn members(&self) -> Result<Vec<(MemberId, AccessLevel)>, GroupError> {
        let result = self.inner.members().await.map(|members| {
            members
                .iter()
                .map(|(actor, access)| (*actor, access.level))
                .collect()
        })?;

        Ok(result)
    }

    // TODO: "actors" method to return the _non-flattened_ actors in a group. This will help to
    // build multi-device applications.
}

#[allow(clippy::from_over_into)]
impl Into<ActorId> for Group {
    fn into(self) -> ActorId {
        self.inner.id()
    }
}

pub struct GroupFuture {
    pub(crate) group_id: ActorId,
    pub(crate) processed: LocalStreamFuture,
}

impl GroupFuture {
    pub fn id(&self) -> ActorId {
        self.group_id
    }
}

impl Future for GroupFuture {
    // TODO: Processing result?
    type Output = Result<(), oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.processed.poll_unpin(cx)
    }
}

#[allow(clippy::from_over_into)]
impl Into<ActorId> for GroupFuture {
    fn into(self) -> ActorId {
        self.group_id
    }
}

/// Event emitted from the group event stream.
///
/// Events are emitted when membership of the group changes due to an action directly changing the root
/// members, or as a result of any child group membership changing.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupEvent {
    /// The current group members.
    pub members: Vec<(ActorId, AccessLevel)>,

    /// The current actor members (can contain individuals and groups).
    pub actors: Vec<(GroupActor, AccessLevel)>,

    /// The inner group event.
    ///
    /// This event may be targeting a child group and contains additionally meta information
    /// regarding the exact change that occurred.
    pub inner: InnerGroupEvent,
}

/// Convert a p2panda_spaces::GroupEvent into p2panda::GroupEvent.
pub fn to_group_event(group_id: GroupId, event: InnerGroupEvent) -> GroupEvent {
    let (members, actors) = if group_id == event.group_id() {
        (
            event.context().members.clone(),
            event.context().actors.clone(),
        )
    } else {
        let GroupContext {
            effected_group_members,
            effected_group_actors,
            ..
        } = event.context();
        let members = effected_group_members
            .get(&group_id)
            .cloned()
            .unwrap_or_default();

        let actors = effected_group_actors
            .get(&group_id)
            .cloned()
            .unwrap_or_default();
        (members, actors)
    };

    GroupEvent {
        members: members.iter().map(to_member).collect(),
        actors: actors.iter().map(to_actor).collect(),
        inner: event,
    }
}

fn to_member(member: &(VerifyingKey, Access<AuthCapabilities>)) -> (VerifyingKey, AccessLevel) {
    (member.0, member.1.level)
}

fn to_actor(
    member: &(p2panda_spaces::GroupActor, Access<AuthCapabilities>),
) -> (GroupActor, AccessLevel) {
    (
        GroupActor {
            id: member.0.id(),
            group: member.0.is_group(),
        },
        member.1.level,
    )
}

#[derive(Debug, Error)]
pub enum GroupError {
    #[error(transparent)]
    Space(#[from] InnerGroupError),

    #[error(transparent)]
    Processor(#[from] ProcessorError),

    #[error(transparent)]
    Manager(#[from] SpacesManagerError),

    #[error(transparent)]
    Import(#[from] ImportError),

    #[error(transparent)]
    CreateStream(#[from] CreateStreamError),
}
