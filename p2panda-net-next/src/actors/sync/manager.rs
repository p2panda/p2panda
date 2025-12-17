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

const RETRY_RATE_SECS: u64 = 5;

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
    /// Initiate a sync session with this peer over the given topic and add them to the active
    /// sync set.
    Initiate {
        node_id: NodeId,
        topic: TopicId,
        live_mode: bool,
    },

    /// Accept a sync session on this connection.
    ///
    /// Do not add the node to our active sync set.
    Accept {
        node_id: NodeId,
        topic: TopicId,
        live_mode: bool,
        connection: Connection,
    },

    /// Send newly published data to all sync sessions running over the given topic.
    Publish { topic: TopicId, data: T },

    /// Close all active sync sessions running over the given topic.
    ///
    /// Additionally empty the current active sync set
    CloseAll { topic: TopicId },

    /// Close all active sync sessions running with the given node id and topic.
    ///
    /// Additionally remove them from the active sync set.
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
    active_sync_set: HashSet<NodeId>,
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
        // manager actor itself terminating.
        //
        // @TODO: We want to link the poller actor so it gets terminated even on a critical
        // failure.
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
            active_sync_set: HashSet::default(),
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

                state.active_sync_set.insert(node_id);
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
                    let _ = handle.send(ToSync::Payload(data.clone())).await;
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
                }
                state.active_sync_set.drain();
            }
            ToSyncManager::Close { node_id, topic } => {
                state.active_sync_set.remove(&node_id);
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

                        let _ = handle.send(ToSync::Close).await;
                    }
                };
            }
        }
        Ok(())
    }

    // Handle supervision events from sync session and poller actors.
    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorTerminated(actor, _, _) => {
                if let Some(name) = SyncSessionName::from_actor_cell(&actor) {
                    debug!(%name.session_id, topic = state.topic.fmt_short(), "sync session terminated");
                    Self::drop_session(state, name.session_id);
                } else {
                    let actor_id = actor.get_id();
                    debug!(%actor_id, topic = state.topic.fmt_short(), "sync poller terminated");
                }
            }
            SupervisionEvent::ActorFailed(actor, err) => {
                if let Some(name) = SyncSessionName::from_actor_cell(&actor) {
                    warn!(%name.session_id, topic = state.topic.fmt_short(), "sync session failed: {}", err);

                    // Retrieve the node id and current sessions from the node session map.
                    let Some(remote_node_id) =
                        state
                            .node_session_map
                            .iter()
                            .find_map(|(node_id, sessions)| {
                                if sessions.contains(&name.session_id) {
                                    Some(*node_id)
                                } else {
                                    None
                                }
                            })
                    else {
                        // If it wasn't present then it means we no longer want to sync with this
                        // node, clear up any session state and return.
                        Self::drop_session(state, name.session_id);
                        return Ok(());
                    };

                    // Clear up any state from the failed session.
                    Self::drop_session(state, name.session_id);

                    // If this node isn't in our active peer set then don't re-initiate sync.
                    if !state.active_sync_set.contains(&remote_node_id) {
                        return Ok(());
                    }

                    let current_sessions = state
                        .node_session_map
                        .get(&remote_node_id)
                        .cloned()
                        .unwrap_or_default();

                    // If there's another session running then we don't need to re-initiate sync.
                    if !current_sessions.is_empty() {
                        return Ok(());
                    }

                    // Send a message to initiate a new session with the remote node.
                    debug!(
                        remote = remote_node_id.fmt_short(),
                        topic = state.topic.fmt_short(),
                        "re-initiate sync after failed session: {}",
                        err
                    );
                    let topic = state.topic;
                    let _ = myself
                        .send_after(Duration::from_secs(RETRY_RATE_SECS), move || {
                            ToSyncManager::Initiate {
                                node_id: remote_node_id,
                                topic,
                                // @TODO: for now we default to live-mode is true but we should rather
                                // retrieve this state from the failed sync session.
                                live_mode: true,
                            }
                        })
                        .await;
                } else {
                    let actor_id = actor.get_id();
                    warn!(%actor_id, topic = state.topic.fmt_short(), "sync poller failed: {}", err);
                }
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
