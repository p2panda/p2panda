// SPDX-License-Identifier: MIT OR Apache-2.0

//! Poll the sync manager for events and forward them to all subscribers.
use std::fmt::Debug;
use std::marker::PhantomData;

use futures_util::{Stream, StreamExt};
use p2panda_sync::FromSync;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tokio::sync::broadcast;

pub enum ToSyncPoller {
    /// Wait for an event from the sync manager.
    WaitForEvent,
}

pub struct SyncPollerState<St, Ev> {
    stream: St,
    sender: broadcast::Sender<FromSync<Ev>>,
}

pub struct SyncPoller<S, Ev> {
    _marker: PhantomData<(S, Ev)>,
}

impl<St, Ev> Default for SyncPoller<St, Ev> {
    fn default() -> Self {
        Self {
            _marker: Default::default(),
        }
    }
}

impl<St, Ev> ThreadLocalActor for SyncPoller<St, Ev>
where
    St: Stream<Item = FromSync<Ev>> + Send + Unpin + 'static,
    Ev: Debug + Send + 'static,
{
    type State = SyncPollerState<St, Ev>;

    type Msg = ToSyncPoller;

    type Arguments = (St, broadcast::Sender<FromSync<Ev>>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (stream, sender) = args;

        // Invoke the handler to wait for the first stream event.
        let _ = myself.cast(ToSyncPoller::WaitForEvent);

        Ok(SyncPollerState { stream, sender })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // We need to keep polling next() on the stream in order for the manager to process and
        // return events coming from running sync sessions. We then forward these events onto all
        // subscribers.
        while let Some(event) = state.stream.next().await {
            state.sender.send(event).map_err(|err| err.to_string())?;
        }

        Ok(())
    }
}
