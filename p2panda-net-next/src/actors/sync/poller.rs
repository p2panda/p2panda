// SPDX-License-Identifier: MIT OR Apache-2.0

//! Listen for messages from the user and forward them to the gossip sender.
use std::fmt::Debug;
use std::marker::PhantomData;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use p2panda_sync::FromSync;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tokio::sync::broadcast;

use crate::actors::ActorNamespace;

pub enum ToSyncPoller {
    /// Wait for a message on the gossip sync channel.
    WaitForMessage,
}

pub struct SyncPollerState<S, E> {
    stream: S,
    sender: broadcast::Sender<FromSync<E>>,
}

pub struct SyncPoller<S, E> {
    _phantom: PhantomData<(S, E)>,
}

impl<S, E> Default for SyncPoller<S, E> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<S, E> ThreadLocalActor for SyncPoller<S, E>
where
    S: Stream<Item = FromSync<E>> + Send + Unpin + 'static,
    E: Debug + Send + Sync + 'static,
{
    type State = SyncPollerState<S, E>;

    type Msg = ToSyncPoller;

    type Arguments = (ActorNamespace, S, broadcast::Sender<FromSync<E>>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (_, stream, sender) = args;

        // Invoke the handler to wait for the first stream event.
        let _ = myself.cast(ToSyncPoller::WaitForMessage);

        Ok(SyncPollerState { stream, sender })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // We need to keep polling next() on the stream in order for the manager to process and
        // return events coming from running sync sessions. We then forward these events onto all
        // subscribers.
        while let Some(event) = state.stream.next().await {
            state.sender.send(event)?;
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
        let _ = myself.cast(ToSyncPoller::WaitForMessage);

        Ok(())
    }
}
