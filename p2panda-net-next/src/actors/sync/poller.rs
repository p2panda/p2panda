// SPDX-License-Identifier: MIT OR Apache-2.0

//! Listen for messages from the user and forward them to the gossip sender.
use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_sync::SyncManagerEvent;
use p2panda_sync::traits::{Protocol, SyncManager as SyncManagerTrait};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};

use crate::actors::ActorNamespace;

pub enum ToSyncPoller {
    /// Wait for a message on the gossip sync channel.
    WaitForMessage,
}

pub struct SyncPollerState<T, M>
where
    M: SyncManagerTrait<T> + Send + 'static,
{
    manager: Arc<Mutex<M>>,
    sender: broadcast::Sender<SyncManagerEvent<T, <M::Protocol as Protocol>::Event>>,
}

pub struct SyncPoller<T, M> {
    _phantom: PhantomData<(T, M)>,
}

impl<T, M> Default for SyncPoller<T, M> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<T, M> ThreadLocalActor for SyncPoller<T, M>
where
    M: SyncManagerTrait<T> + Send + 'static,
    M::Config: Clone + Send + Sync + 'static,
    M::Error: StdError + Send + Sync + 'static,
    <M::Protocol as Protocol>::Event: Debug + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    type State = SyncPollerState<T, M>;
    type Msg = ToSyncPoller;
    type Arguments = (
        ActorNamespace,
        Arc<Mutex<M>>,
        broadcast::Sender<SyncManagerEvent<T, <M::Protocol as Protocol>::Event>>,
    );

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (_, manager, sender) = args;

        // Invoke the handler to wait for the first manager event.
        let _ = myself.cast(ToSyncPoller::WaitForMessage);

        Ok(SyncPollerState { manager, sender })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // We need to keep polling next_event() in order for the manager to process and return
        // events coming from running sync sessions. We then forward these events onto all
        // subscribers.

        // @TODO(sam): we get a lock here every time a message is emitted from the manager, as
        // this is a busy channel (all sync events for all sessions) it would be more efficient to
        // get the lock once and loop over all available messages. This however would block any
        // other access to the manager from the manager actor, which is needed for creating new
        // sessions. It's possible changes can be made to the manager trait and implementation to
        // allow shared access to the manager (interior mutability). Or we decide that locking
        // while draining the event queue is actually ok.
        let mut manager = state.manager.lock().await;
        match manager.next_event().await {
            Ok(Some(event)) => {
                state.sender.send(event)?;
            }
            Ok(None) => {
                // No events on the stream right now
            }
            Err(err) => {
                // An error occurred receiving and processing the next manager event.
                return Err(Box::new(err));
            }
        }
        // Invoke the handler to wait for the next message on the receiver.
        let _ = myself.cast(ToSyncPoller::WaitForMessage);

        Ok(())
    }
}
