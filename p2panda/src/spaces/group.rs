// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{FutureExt, Stream};
use p2panda_auth::{Access, AccessLevel};
use p2panda_spaces::{ActorId, GroupId, GroupsContext, MemberId};
use thiserror::Error;
use tokio::sync::{broadcast, oneshot};
use tokio::task::AbortHandle;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::node::CreateStreamError;
use crate::processor::ProcessorError;
use crate::spaces::GroupActor;
use crate::spaces::types::{
    InnerGroup, InnerGroupError, InnerGroupEvent, NoBody, SpacesManagerError,
};
use crate::streams::{ExternalStreamFuture, ImportError, StreamPublisher, SystemEvent};

#[derive(Debug)]
pub struct Group {
    inner: InnerGroup,
    tx: StreamPublisher<NoBody>,
    event_stream_rx: RefCell<broadcast::Receiver<GroupEvent>>,
    event_stream_handle: AbortHandle,
}

impl Drop for Group {
    fn drop(&mut self) {
        self.event_stream_handle.abort();
    }
}

impl Group {
    pub(crate) fn new(
        inner: InnerGroup,
        tx: StreamPublisher<NoBody>,
        mut in_event_stream_rx: broadcast::Receiver<SystemEvent>,
    ) -> Self {
        let (out_event_stream_tx, out_event_stream_rx) = broadcast::channel::<GroupEvent>(256);

        let group_id = inner.id();
        let event_stream_handle = tokio::spawn(async move {
            while let Ok(event) = in_event_stream_rx.recv().await {
                let SystemEvent::Auth(event) = event else {
                    continue;
                };

                if !event.inner.effects(group_id) {
                    continue;
                }

                // @TODO: convert into dedicated GroupEvent.
                let _ = out_event_stream_tx.send(to_group_event(group_id, event.inner));
            }
        });

        Self {
            inner,
            tx,
            event_stream_rx: RefCell::new(out_event_stream_rx),
            event_stream_handle: event_stream_handle.abort_handle(),
        }
    }

    pub fn id(&self) -> ActorId {
        self.inner.id()
    }

    pub fn event_stream(&self) -> impl Stream<Item = GroupEvent> + Send + Unpin + 'static {
        // Make sure we're not re-subscribing and thus dropping all events which might be still in
        // the buffer of the first broadcast receiver instance.
        let stream = self
            .event_stream_rx
            .replace_with(|stream| stream.resubscribe());

        // TODO: Check if we really want to silence broadcast "lagged" errors here?
        let stream = BroadcastStream::new(stream).filter_map(|event| event.ok());

        Box::pin(stream)
    }

    pub async fn add(
        &self,
        actor: impl Into<ActorId>,
        access: AccessLevel,
    ) -> Result<GroupFuture, GroupError> {
        // @TODO: Group events are currently not forwarded to the user. It's not clear which
        // channel these should be sent on, the spaces stream, a stream for the specific group, or
        // a global groups stream.
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
            .import(futures_util::stream::once(async {
                message.into_operation()
            }))
            .await?;

        Ok(GroupFuture {
            processed,
            group_id: self.inner.id(),
        })
    }

    pub async fn remove(&self, actor: impl Into<ActorId>) -> Result<GroupFuture, GroupError> {
        // @TODO: Group events are currently not forwarded to the user. It's not clear which
        // channel these should be sent on, the spaces stream, a stream for the specific group, or
        // a global groups stream.
        let (_, message, _events) = self.inner.remove(actor.into()).await?;

        let processed = self
            .tx
            .import(futures_util::stream::once(async {
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
    pub(crate) processed: ExternalStreamFuture,
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

#[derive(Clone, Debug, PartialEq)]
pub struct GroupEvent {
    pub group_id: ActorId,
    pub members: Vec<(ActorId, AccessLevel)>,
    pub actors: Vec<(GroupActor, AccessLevel)>,
    pub inner: InnerGroupEvent,
}

pub fn to_group_event(group_id: GroupId, event: InnerGroupEvent) -> GroupEvent {
    let GroupsContext {
        effected_members,
        effected_actors,
        ..
    } = event.context();

    let members = effected_members
        .get(&group_id)
        .map_or_else(Vec::new, |members| {
            members
                .iter()
                .map(|(id, access)| (*id, access.level))
                .collect()
        });

    let actors = effected_actors
        .get(&group_id)
        .map_or_else(Vec::new, |members| {
            members
                .iter()
                .map(|(actor, access)| {
                    (
                        GroupActor {
                            id: actor.id(),
                            group: actor.is_group(),
                        },
                        access.level,
                    )
                })
                .collect()
        });

    GroupEvent {
        group_id,
        members,
        actors,
        inner: event,
    }
}
