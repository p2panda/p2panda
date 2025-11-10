// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::pin::Pin;
use std::time::Duration;

use futures_util::{Sink, SinkExt};
use iroh::endpoint::Connection;
use iroh::protocol::ProtocolHandler;
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::random_walk::{RandomWalker, RandomWalkerConfig};
use p2panda_discovery::{DiscoveryResult, traits};
use p2panda_sync::traits::SyncManager as SyncManagerTrait;
use p2panda_sync::{SessionTopicMap, SyncManagerEvent, SyncSessionConfig, ToSync};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::TopicId;
use crate::actors::iroh::register_protocol;
use crate::actors::sync::SYNC_PROTOCOL_ID;
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::utils::to_public_key;

pub const SYNC_MANAGER: &str = "net.sync.manager";

type SessionSink<M, T> = Pin<Box<dyn Sink<ToSync, Error = <M as SyncManagerTrait<T>>::Error>>>;

pub enum ToSyncManager<T> {
    /// Initiate a sync session with this peer over the given topic.
    Initiate { node_id: NodeId, topic: T },

    /// Accept a sync session on this connection.
    Accept {
        node_id: NodeId,
        connection: Connection,
    },

    /// Forward subscription data to all sync sessions running over the given topic.
    SubscriptionData { topic: T, data: Vec<u8> },

    /// Close all active sync sessions running over the now unsubscribed topic.
    Unsubscribe { topic: T },

    /// Close all active sync sessions running with the given node id and topic.
    Close { node_id: NodeId, topic: T },

    /// Schedule a call to manager.next_event() which must occur to "drive" the manager to
    /// process and emit sync events.
    DriveManager,
}

pub struct SyncManagerState<M, S, T>
where
    M: SyncManagerTrait<T>,
{
    args: ApplicationArguments,
    store: S,
    manager: M,
    session_topic_map: SessionTopicMap<T, SessionSink<M, T>>,
    peer_session_map: HashMap<NodeId, HashSet<u64>>,
    next_session_id: u64,
    pool: ThreadLocalActorSpawner,
    _marker: PhantomData<S>,
}

#[derive(Debug)]
pub struct SyncManager<M, S, T> {
    _marker: PhantomData<(M, S, T)>,
}

impl<M, S, T> Default for SyncManager<M, S, T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<M, S, T> ThreadLocalActor for SyncManager<M, S, T>
where
    M: SyncManagerTrait<T> + Send + 'static,
    // @TODO: need these bounds to be able to handle errors coming from sync with ?
    <M as SyncManagerTrait<T>>::Error: StdError + Send + Sync + 'static,
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Debug + Send + Sync + 'static,
    S::Error: StdError + Debug + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    type State = SyncManagerState<M, S, T>;

    type Msg = ToSyncManager<T>;

    type Arguments = (ApplicationArguments, S, M);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, store, manager) = args;
        let pool = ThreadLocalActorSpawner::new();

        // Accept incoming "sync protocol" connection requests.
        register_protocol(
            SYNC_PROTOCOL_ID,
            SyncProtocolHandler {
                manager_ref: myself.clone(),
            },
        )?;

        Ok(SyncManagerState {
            args,
            store,
            manager,
            session_topic_map: SessionTopicMap::default(),
            peer_session_map: HashMap::default(),
            next_session_id: 0,
            pool,
            _marker: PhantomData,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToSyncManager::Initiate { node_id, topic } => {
                let mut config = SyncSessionConfig::default();
                config.topic = Some(topic.clone());
                let session = Self::new_session(state, node_id, config);

                // @TODO: spawn the sync session. Not clear if this should be an actor yet, if we
                // can't tap into the lifetime events (see below) then maybe better to spawn a
                // task on a local set for now. 
            }
            ToSyncManager::Accept {
                node_id,
                connection,
            } => {
                // @TODO: we're missing a way for accepting peers to check the T topic and/or TopicId they get
                // sent during sync is one they are actually subscribed to. Currently they just
                // accept all sync sessions. We need a way to inject some shared subscription
                // state into each sync session so they can check this during protocol execution.
                let config = SyncSessionConfig::default();
                let session = Self::new_session(state, node_id, config);

                // @TODO: spawn the sync session.
            }
            ToSyncManager::SubscriptionData { topic, data } => {
                // Get a handle onto any sync sessions running over the subscription topic and
                // forward on the data.
                let session_ids = state.session_topic_map.sessions(&topic);
                for id in session_ids {
                    let handle = state
                        .session_topic_map
                        .sender_mut(id)
                        .expect("session handle exists");
                    handle.send(ToSync::Payload(data.clone())).await?;
                }
            }
            ToSyncManager::Unsubscribe { topic } => {
                // Get a handle onto any sync sessions running over the subscription topic and
                // send a Close message. The session will send a close message to the remote then
                // immediately drop the session.

                // @TODO: we may want to add a timeout after which we just
                let session_ids = state.session_topic_map.sessions(&topic);
                for id in session_ids {
                    let handle = state
                        .session_topic_map
                        .sender_mut(id)
                        .expect("session handle exists");

                    handle.send(ToSync::Close).await?;
                }
            }
            ToSyncManager::Close { node_id, topic } => {
                /// Close a sync session with a specific remote and topic.
                let session_ids = state.session_topic_map.sessions(&topic);
                for id in session_ids {
                    let session_topic = state.session_topic_map.topic(id).expect("topic to exist");
                    if &topic != session_topic {
                        continue;
                    }
                    let handle = state
                        .session_topic_map
                        .sender_mut(id)
                        .expect("session handle exists");

                    handle.send(ToSync::Close).await?;
                }
            }
            ToSyncManager::DriveManager => {
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
                    Ok(Err(err)) => {
                        // An error occurred receiving and processing the next manager event.
                        return Err(Box::new(err));
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
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(actor) => {
                // @TODO
            }
            SupervisionEvent::ActorTerminated(actor, state, reason) => {
                // @TODO: drop related session handle on manager.
                // @TODO: need the session id to remove the session from manager state mappings. 
                // Self::drop_session(state, session_id);
            }
            SupervisionEvent::ActorFailed(actor, error) => {
                // @TODO
            }
            _ => (),
        }

        Ok(())
    }
}

impl<M, S, T> SyncManager<M, S, T>
where
    M: SyncManagerTrait<T> + Send + 'static,
    <M as SyncManagerTrait<T>>::Error: StdError + Send + Sync + 'static,
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Debug + Send + Sync + 'static,
    S::Error: StdError + Debug + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    /// Initiate a session and update related manager state mappings.
    fn new_session(
        state: &mut SyncManagerState<M, S, T>,
        node_id: NodeId,
        config: SyncSessionConfig<T>,
    ) -> <M as SyncManagerTrait<T>>::Protocol {
        // Get next session id.
        let session_id: u64 = state.next_session_id;
        state.next_session_id += 1;

        // Instantiate the session.
        let session = state.manager.session(session_id, &config);

        // Get a tx sender handle to the session.
        let session_handle = state
            .manager
            .session_handle(session_id)
            .expect("we just created this session");

        // Register the session on the manager state as "accepting" or "initiated".
        match config.topic {
            Some(topic) => {
                state
                    .session_topic_map
                    .insert_with_topic(session_id, topic, session_handle);
            }
            None => {
                state
                    .session_topic_map
                    .insert_accepting(session_id, session_handle);
            }
        }

        // Associate the session with the given node id on manager state.
        state
            .peer_session_map
            .entry(node_id)
            .or_default()
            .insert(session_id);

        // Return the session.
        session
    }

    /// Remove a session from all manager state mappings.
    fn drop_session(state: &mut SyncManagerState<M, S, T>, session_id: u64) {
        // @TODO: drop the session from all mappings.
    }
}

#[derive(Debug)]
struct SyncProtocolHandler<T> {
    manager_ref: ActorRef<ToSyncManager<T>>,
}

impl<T> ProtocolHandler for SyncProtocolHandler<T>
where
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    async fn accept(
        &self,
        connection: iroh::endpoint::Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        let node_id = to_public_key(connection.remote_id());
        self.manager_ref
            .send_message(ToSyncManager::Accept {
                node_id,
                connection,
            })
            .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;
        //         let (_, handle) = SyncSession::spawn_linked(
        //             None,
        //             (
        //                 to_public_key(connection.remote_id()),
        //                 self.store.clone(),
        //                 self.manager_ref.clone(),
        //                 SyncSessionArguments::Accept { connection },
        //             ),
        //             self.manager_ref.clone().into(),
        //             self.pool.clone(),
        //         )
        //         .await
        //         .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;
        //
        // Wait until discovery session ended (failed or successful).
        // handle
        //     .await
        //     .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct SubscriptionInfo<T> {
    _marker: PhantomData<T>,
}

impl<T> SubscriptionInfo<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T> traits::SubscriptionInfo<T> for SubscriptionInfo<T> {
    type Error = SubscriptionInfoError;

    async fn subscribed_topics(&self) -> Result<Vec<T>, Self::Error> {
        // @TODO: Call actor which can respond with the currently subscribed topics.
        Ok(vec![])
    }

    async fn subscribed_topic_ids(&self) -> Result<Vec<TopicId>, Self::Error> {
        // @TODO: Call actor which can respond with the currently subscribed topic ids.
        Ok(vec![])
    }
}

#[derive(Debug, Error)]
pub enum SubscriptionInfoError {
    #[error("actor '{0}' is not available")]
    ActorNotAvailable(String),

    #[error("actor '{0}' is not responding to call")]
    ActorNotResponsive(String),
}
