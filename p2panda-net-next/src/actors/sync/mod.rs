// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync actor.
mod listener;
mod receiver;
mod sender;
mod session;

use std::marker::PhantomData;
use std::pin::Pin;
use std::time::Duration;

use futures::Sink;
use p2panda_sync_next::traits::{SyncManager, TopicQuery};
use p2panda_sync_next::{
    SessionTopicMap, SyncManagerEvent, SyncSessionConfig, ToSync as ToSyncManager,
};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tokio::sync::Mutex;

use crate::{NodeId, TopicId};

pub enum ToSync {
    /// Schedule a call to manager.next_event() which must occur to "drive" the manager to
    /// process and emit sync events.
    DriveManager,

    /// Sent when a new peer has been identified whom we should sync with.
    ///
    /// This will trigger the sync manager actor to request a connection to the stated peer, when
    /// this returns the connection a sync session will be initiated.
    SyncWith { node_id: NodeId, topic: TopicId },

    /// Sent in response to a previous request from the manager to establish a sync connection
    /// with a peer.
    SyncAccepted {
        node_id: NodeId,
        topic: TopicId,
        rx: (),
        tx: (),
    },

    /// Sent when a sync connection request has been received by the node.
    ///
    /// The manager can then kick of a sync session as "acceptor". If some work limits have been
    /// reached then it may be that a sync request is just ignored.
    SyncRequest { node_id: NodeId, rx: (), tx: () },
}

impl Message for ToSync {}

type SessionSink<S> = Pin<Box<dyn Sink<ToSyncManager, Error = <S as SyncManager<TopicId>>::Error>>>;

pub struct SyncState<S>
where
    S: SyncManager<TopicId> + 'static,
{
    session_topic_map: SessionTopicMap<TopicId, SessionSink<S>>,
    next_session_id: u64,
    manager: S,
}

#[derive(Debug)]
pub struct Sync<S> {
    _phantom: PhantomData<S>,
}

impl<S> Default for Sync<S> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<S> ThreadLocalActor for Sync<S>
where
    // @TODO: S still needs to be Send because I'm using it as an argument, is there a way around
    // this?
    S: SyncManager<TopicId> + Send + 'static,
{
    type State = SyncState<S>;
    type Msg = ToSync;
    type Arguments = S;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        manager: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(SyncState {
            session_topic_map: SessionTopicMap::default(),
            manager,
            next_session_id: 0,
        })
    }

    async fn post_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToSync::SyncWith { node_id, topic } => {
                // @TODO: request a connection for sync, expect a SyncAccepted in return.
            }
            ToSync::SyncRequest { node_id, rx, tx } => {
                let session_id = state.next_session_id;
                state.next_session_id += 1;

                let config = SyncSessionConfig::default();
                let session = state.manager.session(session_id, &config);

                let session_handle = state
                    .manager
                    .session_handle(session_id)
                    .expect("we just created this session");
                state
                    .session_topic_map
                    .insert_accepting(session_id, session_handle);

                // @TODO: spawn the sync session, we already have a connection so we're good to go.
            }
            ToSync::SyncAccepted {
                node_id,
                topic,
                rx,
                tx,
            } => {
                let session_id = state.next_session_id;
                state.next_session_id += 1;

                let mut config = SyncSessionConfig::default();
                config.topic = Some(topic);
                let session = state.manager.session(state.next_session_id, &config);
                let session_handle = state
                    .manager
                    .session_handle(session_id)
                    .expect("we just created this session");
                state
                    .session_topic_map
                    .insert_with_topic(session_id, topic, session_handle);

                // - spawn the sync session (actor)
            }
            ToSync::DriveManager => {
                // This message variant is for "driving" the manager. We need to keep polling
                // next_event() in order for the manager to progress with it's work, and
                // ultimately we want to forward any events which are returned up to relevant
                // subscribers.

                // @TODO(sam): I'm not convinced this is the right approach yet. The tricky thing
                // is that next_event() is doing "manager work" internally on each call before
                // returning the event. It's not just a simple channel. An alternative approach
                // would be to return a handle from the manager which would be Clone and could be
                // run in it's own task. This would still need to do some "work" (forwarding
                // events between sync sessions), which is where I feel it gets a little strange,
                // as I wouldn't expect that from a "simple" handler.

                let event_fut = state.manager.next_event();
                match tokio::time::timeout(Duration::from_millis(50), event_fut).await {
                    Ok(Ok(Some(event))) => {
                        // If this is a "topic agreed" event then upgrade the related session to "accepted".
                        if let SyncManagerEvent::TopicAgreed { session_id, topic } = &event {
                            state.session_topic_map.accepted(*session_id, topic.clone());
                        }

                        // @TODO: Send the event on to the subscription actor.
                    }
                    Ok(Ok(None)) => {
                        // No events on the stream right now
                    }
                    Ok(error) => {
                        // An error occurred in the call to manager.next_event()
                        panic!("proper error handling")
                    }
                    Err(_) => {
                        // The timeout elapsed, move on to handle the next manager event
                    }
                }
            }
        }
        
        // In every case we send a message to ourselves to once again "drive" the manager.
        myself.send_message(Self::Msg::DriveManager)?;

        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }
}
