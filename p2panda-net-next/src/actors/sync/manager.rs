// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

use futures_channel::mpsc;
use futures_util::{Sink, SinkExt};
use iroh::endpoint::Connection;
use iroh::protocol::ProtocolHandler;
use p2panda_sync::topic_handshake::{
    TopicHandshakeAcceptor, TopicHandshakeEvent, TopicHandshakeMessage,
};
use p2panda_sync::traits::{Protocol, SyncManager as SyncManagerTrait};
use p2panda_sync::{SessionTopicMap, SyncManagerEvent, SyncSessionConfig, ToSync};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, SupervisionEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast};

use crate::TopicId;
use crate::actors::ActorNamespace;
use crate::actors::iroh::register_protocol;
use crate::actors::sync::SYNC_PROTOCOL_ID;
use crate::actors::sync::poller::SyncPoller;
use crate::actors::sync::session::{SyncSession, SyncSessionMessage};
use crate::addrs::NodeId;
use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::utils::to_public_key;

pub const SYNC_MANAGER: &str = "net.sync.manager";

type SessionSink<M, T> = Pin<Box<dyn Sink<ToSync, Error = <M as SyncManagerTrait<T>>::Error>>>;

pub enum ToSyncManager<T> {
    /// Initiate a sync session with this peer over the given topic.
    Initiate {
        node_id: NodeId,
        topic: T,
        live_mode: bool,
    },

    /// Accept a sync session on this connection.
    Accept {
        node_id: NodeId,
        topic: T,
        live_mode: bool,
        connection: Connection,
    },

    /// Send newly published data to all sync sessions running over the given topic.
    Publish { topic: T, data: Vec<u8> },

    /// Close all active sync sessions running over the given topic.
    CloseAll { topic: T },

    /// Close all active sync sessions running with the given node id and topic.
    Close { node_id: NodeId, topic: T },
}

pub struct SyncManagerState<M, T>
where
    M: SyncManagerTrait<T>,
{
    actor_namespace: ActorNamespace,
    #[allow(unused)]
    topic_id: TopicId,
    // @TODO: Would rather refactor the M manager itself to use inner mutability so as to avoid
    // locking access to the whole manager on every read/write.
    manager: Arc<Mutex<M>>,
    session_topic_map: SessionTopicMap<T, SessionSink<M, T>>,
    node_session_map: HashMap<NodeId, HashSet<u64>>,
    next_session_id: u64,
    pool: ThreadLocalActorSpawner,
}

#[derive(Debug)]
pub struct SyncManager<M, T> {
    _marker: PhantomData<(M, T)>,
}

impl<M, T> Default for SyncManager<M, T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<M, T> ThreadLocalActor for SyncManager<M, T>
where
    M: SyncManagerTrait<T> + Send + 'static,
    M::Error: StdError + Send + Sync + 'static,
    M::Protocol: Send + 'static,
    <M::Protocol as Protocol>::Event: Debug + Send + Sync + 'static,
    <M::Protocol as Protocol>::Error: StdError + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    type State = SyncManagerState<M, T>;

    type Msg = ToSyncManager<T>;

    type Arguments = (
        ActorNamespace,
        TopicId,
        M::Config,
        broadcast::Sender<SyncManagerEvent<T, <M::Protocol as Protocol>::Event>>,
    );

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (actor_namespace, topic_id, config, sender) = args;
        let pool = ThreadLocalActorSpawner::new();

        // @TODO: move registering the sync protocol to the stream actor as this is who will be
        // responsible for routing accepted sync requests to the correct manager based on the
        // resolved topic.

        // Accept incoming "sync protocol" connection requests.
        register_protocol(
            SYNC_PROTOCOL_ID,
            SyncProtocolHandler {
                manager_ref: myself.clone(),
            },
            SYNC_MANAGER.to_string(),
        )?;

        let manager = Arc::new(Mutex::new(M::from_config(config)));

        let (_, _) = SyncPoller::<T, M>::spawn_linked(
            None,
            (actor_namespace.clone(), manager.clone(), sender),
            myself.clone().into(),
            pool.clone(),
        )
        .await?;

        Ok(SyncManagerState {
            actor_namespace,
            topic_id,
            manager,
            session_topic_map: SessionTopicMap::default(),
            node_session_map: HashMap::default(),
            next_session_id: 0,
            pool,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToSyncManager::Initiate {
                node_id,
                topic,
                live_mode,
            } => {
                let config = SyncSessionConfig {
                    live_mode,
                    topic: Some(topic.clone()),
                };
                let (session, _) = Self::new_session(state, node_id, topic.clone(), config).await;

                let (actor_ref, _) = SyncSession::<T, M::Protocol>::spawn_linked(
                    None,
                    state.actor_namespace.clone(),
                    myself.clone().into(),
                    state.pool.clone(),
                )
                .await?;

                actor_ref.send_message(SyncSessionMessage::Initiate {
                    node_id,
                    topic,
                    protocol: session,
                })?;
            }
            ToSyncManager::Accept {
                node_id,
                connection,
                topic,
                live_mode,
            } => {
                // @TODO: once topic handshake has been removed from the managers' protocol
                // implementation then we will also set the topic here. For now a redundant round
                // of topic handshake occurs.
                let config = SyncSessionConfig {
                    live_mode,
                    topic: None,
                };
                let (session, _) = Self::new_session(state, node_id, topic, config).await;

                let (actor_ref, _) = SyncSession::<T, M::Protocol>::spawn_linked(
                    None,
                    state.actor_namespace.clone(),
                    myself.clone().into(),
                    state.pool.clone(),
                )
                .await?;

                actor_ref.send_message(SyncSessionMessage::Accept {
                    connection,
                    protocol: session,
                })?;
            }
            ToSyncManager::Publish { topic, data } => {
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
            ToSyncManager::CloseAll { topic } => {
                // Get a handle onto any sync sessions running over the subscription topic and
                // send a Close message. The session will send a close message to the remote then
                // immediately drop the session.
                let session_ids = state.session_topic_map.sessions(&topic);
                for id in session_ids {
                    let handle = state
                        .session_topic_map
                        .sender_mut(id)
                        .expect("session handle exists");
                    handle.send(ToSync::Close).await?;
                    Self::drop_session(state, id);
                }
            }
            ToSyncManager::Close { node_id, topic } => {
                // Close a sync session with a specific remote and topic.
                let node_sessions = state.node_session_map.get(&node_id).cloned();
                if let Some(node_sessions) = node_sessions {
                    let topic_sessions = state.session_topic_map.sessions(&topic);
                    for id in topic_sessions.intersection(&node_sessions) {
                        let session_topic =
                            state.session_topic_map.topic(*id).expect("topic to exist");
                        if &topic != session_topic {
                            continue;
                        }
                        let handle = state
                            .session_topic_map
                            .sender_mut(*id)
                            .expect("session handle exists");

                        handle.send(ToSync::Close).await?;
                        Self::drop_session(state, *id);
                    }
                };
            }
        }
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(_actor) => {
                // @TODO
            }
            SupervisionEvent::ActorTerminated(_actor, _state, _reason) => {
                // @TODO: drop related session handle on manager.
                // @TODO: need the session id to remove the session from manager state mappings.
                // @TODO: have the session id as the actor suffix and parse it out here.
                // Self::drop_session(state, session_id);
            }
            SupervisionEvent::ActorFailed(_actor, _error) => {
                // @TODO: have the session id as the actor suffix and parse it out here.
                // Self::drop_session(state, session_id);
            }
            _ => (),
        }

        Ok(())
    }
}

impl<M, T> SyncManager<M, T>
where
    M: SyncManagerTrait<T> + Send + 'static,
    <M as SyncManagerTrait<T>>::Error: StdError + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    /// Initiate a session and update related manager state mappings.
    async fn new_session(
        state: &mut SyncManagerState<M, T>,
        node_id: NodeId,
        topic: T,
        config: SyncSessionConfig<T>,
    ) -> (<M as SyncManagerTrait<T>>::Protocol, u64) {
        // Get next session id.
        let session_id: u64 = state.next_session_id;
        state.next_session_id += 1;

        // Instantiate the session.
        let mut manager = state.manager.lock().await;
        let session = manager.session(session_id, &config).await;

        // Get a tx sender handle to the session.
        let session_handle = manager
            .session_handle(session_id)
            .expect("we just created this session");

        // Register the session on the manager state.
        //
        // @NOTE: We don't distinguish between "accepting" and "accepted" sync sessions as in both
        // cases the topic is known thanks to the topic handshake already having been performed.
        state
            .session_topic_map
            .insert_with_topic(session_id, topic, session_handle);

        // Associate the session with the given node id on manager state.
        state
            .node_session_map
            .entry(node_id)
            .or_default()
            .insert(session_id);

        // Return the session.
        (session, session_id)
    }

    /// Remove a session from all manager state mappings.
    fn drop_session(state: &mut SyncManagerState<M, T>, id: u64) {
        state.session_topic_map.drop(id);
        state.node_session_map.iter_mut().for_each(|(_, sessions)| {
            sessions.remove(&id);
        });
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
        let (tx, rx) = connection.accept_bi().await?;

        // As we are accepting a sync session here we don't yet know the topic which the initiator
        // choses themselves. This "topic handshake" step takes place here before accepting the
        // actual sync session. We may choose to reject the sync session if the handshake resolves
        // to a topic we aren't subscribed to.

        // Establish bi-directional QUIC stream as part of the direct connection and use CBOR
        // encoding for message framing.
        let mut tx = into_cbor_sink::<TopicHandshakeMessage<T>, _>(tx);
        let mut rx = into_cbor_stream::<TopicHandshakeMessage<T>, _>(rx);

        // Channels for sending and receiving protocol events.
        //
        // @NOTE: We don't need to observe these events here as the topic is returned as output
        // when the protocol completes, so these channels are actually only just to satisfy the
        // API.
        let (event_tx, _event_rx) = mpsc::channel::<TopicHandshakeEvent<T>>(128);
        let protocol = TopicHandshakeAcceptor::new(event_tx);
        let topic = protocol
            .run(&mut tx, &mut rx)
            .await
            .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;

        // We know the topic now and send an accept message to the sync manager.
        //
        // @TODO: this will go to the stream actor and then be routed to the correct sync manager.
        self.manager_ref
            .send_message(ToSyncManager::Accept {
                topic,
                node_id,
                connection,
                live_mode: true,
            })
            .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;

        Ok(())
    }
}
