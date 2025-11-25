// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;

use futures_util::{Sink, SinkExt};
use iroh::endpoint::Connection;
use p2panda_sync::traits::SyncManager as SyncManagerTrait;
use p2panda_sync::{FromSync, SessionTopicMap, SyncSessionConfig, ToSync};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, SupervisionEvent};
use tokio::sync::broadcast;
use tokio::time::Duration;
use tracing::{debug, warn};

use crate::TopicId;
use crate::actors::ActorNamespace;
use crate::actors::sync::SyncSessionName;
use crate::actors::sync::poller::{SyncPoller, ToSyncPoller};
use crate::actors::sync::session::{SyncSession, SyncSessionId, SyncSessionMessage};
use crate::addrs::NodeId;
use crate::utils::ShortFormat;

type SessionSink<M> = Pin<
    Box<
        dyn Sink<
                ToSync<<M as SyncManagerTrait<TopicId>>::Message>,
                Error = <M as SyncManagerTrait<TopicId>>::Error,
            >,
    >,
>;

#[derive(Debug)]
pub enum ToSyncManager<T> {
    /// Initiate a sync session with this peer over the given topic.
    Initiate {
        node_id: NodeId,
        topic: TopicId,
        live_mode: bool,
    },

    /// Accept a sync session on this connection.
    Accept {
        node_id: NodeId,
        topic: TopicId,
        live_mode: bool,
        connection: Connection,
    },

    /// Send newly published data to all sync sessions running over the given topic.
    Publish { topic: TopicId, data: T },

    /// Close all active sync sessions running over the given topic.
    CloseAll { topic: TopicId },

    /// Close all active sync sessions running with the given node id and topic.
    Close { node_id: NodeId, topic: TopicId },
}

pub struct SyncManagerState<M>
where
    M: SyncManagerTrait<TopicId>,
{
    actor_namespace: ActorNamespace,
    #[allow(unused)]
    topic: TopicId,
    manager: M,
    session_topic_map: SessionTopicMap<TopicId, SessionSink<M>>,
    node_session_map: HashMap<NodeId, HashSet<SyncSessionId>>,
    next_session_id: SyncSessionId,
    sync_poller_actor: ActorRef<ToSyncPoller>,
    pool: ThreadLocalActorSpawner,
}

#[derive(Debug)]
pub struct SyncManager<M> {
    _marker: PhantomData<M>,
}

impl<M> Default for SyncManager<M> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<M> ThreadLocalActor for SyncManager<M>
where
    M: SyncManagerTrait<TopicId> + Debug + Send + 'static,
{
    type State = SyncManagerState<M>;

    type Msg = ToSyncManager<M::Message>;

    type Arguments = (
        ActorNamespace,
        TopicId,
        M::Config,
        broadcast::Sender<FromSync<M::Event>>,
    );

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (actor_namespace, topic, config, sender) = args;
        let pool = ThreadLocalActorSpawner::new();

        let mut manager = M::from_config(config);
        let event_stream = manager.subscribe();

        // The sync poller actor lives as long as the manager and only terminates due to the
        // manager actor itself terminating. Therefore no supervision is required.
        let (sync_poller_actor, _) = SyncPoller::spawn(
            None,
            (actor_namespace.clone(), event_stream, sender),
            pool.clone(),
        )
        .await?;

        Ok(SyncManagerState {
            actor_namespace,
            topic,
            manager,
            session_topic_map: SessionTopicMap::default(),
            node_session_map: HashMap::default(),
            next_session_id: 0,
            sync_poller_actor,
            pool,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Drain the sync poller to ensure that all sync session messages are forwarded before it
        // is shut down. A timeout is included to ensure that the drain call cannot wait forever.
        state
            .sync_poller_actor
            .drain_and_wait(Some(Duration::from_millis(5000)))
            .await?;

        Ok(())
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
                debug!(
                    remote_node_id = %node_id.fmt_short(),
                    topic = %topic.fmt_short(),
                    %live_mode,
                    "initiate sync session"
                );

                let config = SyncSessionConfig {
                    topic,
                    remote: node_id,
                    live_mode,
                };
                let (session, id) = Self::new_session(state, node_id, topic, config).await;
                let name = Some(SyncSessionName::new(id).to_string(&state.actor_namespace));
                let (actor_ref, _) = SyncSession::<M::Protocol>::spawn_linked(
                    name,
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
                debug!(
                    remote_node_id = %node_id.fmt_short(),
                    topic = %topic.fmt_short(),
                    %live_mode,
                    "accept sync session"
                );

                let config = SyncSessionConfig {
                    topic,
                    remote: node_id,
                    live_mode,
                };
                let (session, id) = Self::new_session(state, node_id, topic, config).await;
                let name = Some(SyncSessionName::new(id).to_string(&state.actor_namespace));
                let (actor_ref, _) = SyncSession::<M::Protocol>::spawn_linked(
                    name,
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
                    let _ = handle.send(ToSync::Close).await;
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
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorTerminated(actor, _, _) => {
                let name = SyncSessionName::from_actor_cell(&actor);
                debug!("sync session {} terminated", name.session_id);
                Self::drop_session(state, name.session_id);
            }
            SupervisionEvent::ActorFailed(actor, err) => {
                let name = SyncSessionName::from_actor_cell(&actor);
                warn!("sync session {} failed: {}", name.session_id, err);
                Self::drop_session(state, name.session_id);
            }
            _ => (),
        }

        Ok(())
    }
}

impl<M> SyncManager<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
    <M as SyncManagerTrait<TopicId>>::Error: StdError + Send + Sync + 'static,
{
    /// Initiate a session and update related manager state mappings.
    async fn new_session(
        state: &mut SyncManagerState<M>,
        node_id: NodeId,
        topic: TopicId,
        config: SyncSessionConfig<TopicId>,
    ) -> (<M as SyncManagerTrait<TopicId>>::Protocol, SyncSessionId) {
        // Get next session id.
        let session_id: SyncSessionId = state.next_session_id;
        state.next_session_id += 1;

        // Instantiate the session.
        let session = state.manager.session(session_id, &config).await;

        // Get a tx sender handle to the session.
        let session_handle = state
            .manager
            .session_handle(session_id)
            .await
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
    fn drop_session(state: &mut SyncManagerState<M>, id: SyncSessionId) {
        state.session_topic_map.drop(id);
        state.node_session_map.iter_mut().for_each(|(_, sessions)| {
            sessions.remove(&id);
        });
    }
}
