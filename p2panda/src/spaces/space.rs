// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{FutureExt, Stream, StreamExt};
use p2panda_auth::{Access, AccessLevel};
use p2panda_core::cbor::{EncodeError, encode_cbor};
use p2panda_spaces::{ActorId, MemberId, SpaceContext, SpaceId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::oneshot;

use crate::node::CreateStreamError;
use crate::spaces::types::{InnerSpace, InnerSpaceError, SpacesManagerError};
use crate::streams::{
    ExternalStreamFuture, ImportError, ProcessedOperation, Source, StreamEvent, StreamPublisher,
    StreamSubscription,
};

/// Wraps topic stream and returns the pub/sub pair of a more specialised spaces stream.
pub(crate) fn spaces_stream<M>(
    inner: InnerSpace,
    tx: StreamPublisher<M>,
    rx: StreamSubscription<M>,
) -> (Space<M>, SpaceSubscription<M>) {
    (Space { inner, tx }, SpaceSubscription { rx })
}

// TODO: We need a way to automatically publish key bundles (if configured, it should also be an
// option to _not_ do that to only allow initial key agreement through side channels which is more
// private).
//
// TODO: Automatically repair spaces (detect missing group changes and apply them to affected
// spaces), ideally with a throttle logic.
#[derive(Debug)]
pub struct Space<M> {
    inner: InnerSpace,
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
        // TODO: We'll remove custom `M` types in the future, users will only provide bytes on this
        // level.
        let body_bytes = encode_cbor(&message)?;

        // TODO: This should _not_ encrypt the message (yet), instead the processor will deal with
        // it. The only thing we want from here is a marker that this will be encrypted.
        //
        // We could also handle this outside of p2panda-spaces, simply by coming up with an argument
        // in the extensions for the spaces processor in p2panda-stream.
        let (_, message) = self.inner.publish(&body_bytes).await?;

        let processed = self
            .tx
            .import(futures_util::stream::once(async {
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
    ) -> Result<SpaceFuture, SpaceError> {
        let (_, _, auth_message, space_message) = self
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
            .import(futures_util::stream::iter([
                auth_message.into_operation(),
                space_message.into_operation(),
            ]))
            .await?;

        Ok(SpaceFuture {
            processed,
            space_id: self.inner.id(),
        })
    }

    pub async fn remove(&self, actor: impl Into<ActorId>) -> Result<SpaceFuture, SpaceError> {
        let (_, _, auth_message, space_message) = self.inner.remove(actor.into()).await?;

        let processed = self
            .tx
            .import(futures_util::stream::iter([
                auth_message.into_operation(),
                space_message.into_operation(),
            ]))
            .await?;

        Ok(SpaceFuture {
            processed,
            space_id: self.inner.id(),
        })
    }

    pub async fn members(&self) -> Result<Vec<(MemberId, AccessLevel)>, SpaceError> {
        let result = self.inner.members().await.map(|members| {
            members
                .iter()
                .map(|(actor, access)| (*actor, access.level.clone()))
                .collect()
        })?;

        Ok(result)
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
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    type Item = SpaceEvent<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let result = self.rx.poll_next_unpin(cx);

            let event = match result {
                Poll::Ready(Some(event)) => event,
                Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            };

            // TODO: Properly convert to SpaceEvent.
            match event {
                StreamEvent::Processed { operation, source } => {
                    return Poll::Ready(Some(SpaceEvent::Processed {
                        operation: Box::new(operation),
                        source,
                    }));
                }
                _ => {
                    continue;
                }
            }
        }
    }
}

#[derive(Debug)]
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
    pub(crate) processed: ExternalStreamFuture,
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
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum PublishSpaceError {
    #[error(transparent)]
    Space(#[from] InnerSpaceError),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Import(#[from] ImportError),
}
