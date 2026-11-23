// SPDX-License-Identifier: MIT OR Apache-2.0

//! Topic manager actor.
//!
//! The topic manager holds state for all sync sessions associated with a single topic. It provides
//! a means of initating new sync sessions, accepting inbound sync sessions and publishing messages
//! to all active sync sessions for the associated topic.
//!
//! A separate topic manager actor is spawned by the sync manager for each topic of interest.

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;

use iroh::endpoint::Connection;
use p2panda_core::traits::Digest;
use p2panda_core::{Hash, Topic};
use p2panda_sync::dedup::DeduplicationBuffer;
use p2panda_sync::{FromSync, ToSync};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorId, ActorProcessingErr, ActorRef, SupervisionEvent};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Duration;
use tracing::{debug, warn};

use crate::iroh_endpoint::Endpoint;
use crate::sync::actors::session::{SessionState, SyncSession, SyncSessionId, SyncSessionMessage};
use crate::sync::session::{SessionBuilder, SyncSessionItem};
use crate::sync::sessions::LiveSessions;
use crate::utils::ShortFormat;
use crate::{NodeId, ProtocolId};

const RETRY_RATE: Duration = Duration::from_secs(5);

/// Capacity of each session's channel for payloads to send to the remote in live mode.
const LIVE_BUFFER: usize = 1024;

#[derive(Debug)]
pub enum ToTopicManager<M, E> {
    /// Initiate a sync session with this peer over the given topic
    Initiate {
        node_id: NodeId,
        topic: Topic,
        live_mode: bool,
    },

    /// Accept a sync session on this connection.
    Accept {
        node_id: NodeId,
        topic: Topic,
        live_mode: bool,
        connection: Connection,
    },

    /// Retry sync with this peer after a failed session.
    Retry { node_id: NodeId, live_mode: bool },

    /// Send newly published data to all sync sessions running over the given topic.
    Publish(M),

    /// Close all active sync sessions running over the given topic. This essentially shuts down
    /// the whole manager.
    CloseAll(oneshot::Sender<()>),

    /// Close all active sync sessions running with the given node id.
    Close {
        node_id: NodeId,
        reply: oneshot::Sender<()>,
    },

    /// Event emitted by one of the sync sessions.
    SessionItem(FromSync<E>),
}

pub struct TopicManagerState<S>
where
    S: SessionBuilder,
{
    /// Topic of this manager instance.
    topic: Topic,

    /// Session builder.
    session_builder: S,

    /// ALPN used for iroh connection establishment.
    protocol_id: ProtocolId,

    /// Nodes with which we are currently syncing.
    ///
    /// If a sync session fails with one of these nodes we will attempt a retry.
    active_sync_set: HashSet<NodeId>,

    /// Session run by each session actor, until the actor has stopped.
    sessions: HashMap<ActorId, SessionInfo>,

    /// Id of the next session.
    next_session_id: SyncSessionId,

    /// Iroh endpoint.
    endpoint: Endpoint,

    /// Local pool.
    pool: ThreadLocalActorSpawner,

    /// All currently active live-mode sessions.
    live_sessions: LiveSessions<S::Operation>,

    /// Topic event stream sender.
    event_tx: broadcast::Sender<FromSync<S::Event>>,

    /// Deduplication buffer for this topic's event stream.
    received: DeduplicationBuffer<Hash>,
}

/// Info for an active sync session.
struct SessionInfo {
    session_id: SyncSessionId,
    remote: NodeId,
    live_mode: bool,
}

#[derive(Debug)]
pub struct TopicManager<S> {
    _marker: PhantomData<S>,
}

impl<S> Default for TopicManager<S> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S> ThreadLocalActor for TopicManager<S>
where
    S: SessionBuilder,
{
    type State = TopicManagerState<S>;

    type Msg = ToTopicManager<S::Operation, S::Event>;

    type Arguments = (
        ProtocolId,
        Topic,
        S,
        broadcast::Sender<FromSync<S::Event>>,
        Endpoint,
    );

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (protocol_id, topic, session_builder, event_tx, endpoint) = args;
        let pool = ThreadLocalActorSpawner::new();

        Ok(TopicManagerState {
            topic,
            session_builder,
            protocol_id,
            active_sync_set: HashSet::new(),
            sessions: HashMap::new(),
            next_session_id: 0,
            endpoint,
            pool,
            live_sessions: LiveSessions::default(),
            event_tx,
            received: Default::default(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToTopicManager::Initiate {
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

                let (actor_ref, live_rx) =
                    Self::spawn_session(&myself, state, &node_id, live_mode).await?;

                actor_ref.send_message(SyncSessionMessage::Initiate {
                    protocol_id: state.protocol_id.clone(),
                    live_rx,
                })?;
            }
            ToTopicManager::Retry { node_id, live_mode } => {
                // If this node was removed from the active sync set we skip retrying.
                if !state.active_sync_set.contains(&node_id) {
                    debug!(
                        remote = %node_id.fmt_short(),
                        topic = %state.topic.fmt_short(),
                        %live_mode,
                        "skip re-initiate sync: node no longer in active set"
                    );
                    return Ok(());
                };

                // If there's another session running then we don't need to re-initiate sync.
                if state.sessions.values().any(|info| info.remote == node_id) {
                    debug!(
                        remote = %node_id.fmt_short(),
                        topic = %state.topic.fmt_short(),
                        %live_mode,
                        "skip re-initiate sync: other sync sessions already running"
                    );
                    return Ok(());
                }

                debug!(
                    remote = %node_id.fmt_short(),
                    topic = %state.topic.fmt_short(),
                    %live_mode,
                    "re-initiate sync after failed session"
                );

                let (actor_ref, live_rx) =
                    Self::spawn_session(&myself, state, &node_id, live_mode).await?;

                actor_ref.send_message(SyncSessionMessage::Initiate {
                    protocol_id: state.protocol_id.clone(),
                    live_rx,
                })?;
            }
            ToTopicManager::Accept {
                node_id,
                connection,
                topic,
                live_mode,
            } => {
                debug!(
                    remote = %node_id.fmt_short(),
                    topic = %topic.fmt_short(),
                    %live_mode,
                    "accept sync session"
                );

                let (actor_ref, live_rx) =
                    Self::spawn_session(&myself, state, &node_id, live_mode).await?;

                actor_ref.send_message(SyncSessionMessage::Accept {
                    connection,
                    live_rx,
                })?;
            }
            ToTopicManager::Publish(operation) => {
                state.live_sessions.send(&operation).await;
            }
            ToTopicManager::SessionItem(sync_item) => {
                if let Some(payload) = sync_item.event.operation() {
                    state
                        .live_sessions
                        .received(sync_item.session_id, payload)
                        .await;

                    if !state.received.insert(payload.hash()) {
                        return Ok(());
                    }
                }

                // Fails only when there are no subscribers.
                let _ = state.event_tx.send(sync_item);
            }
            ToTopicManager::CloseAll(reply) => {
                // Send a Close message to all sync sessions running over the subscription topic.
                // The session will send a close message to the remote then immediately drop the
                // session.
                state.live_sessions.close_all().await;

                myself
                    .drain_children_and_wait(Some(Duration::from_millis(5000)))
                    .await;

                for node_id in state.active_sync_set.drain() {
                    debug!(
                        topic = state.topic.fmt_short(),
                        "removed node from active sync set: {}",
                        node_id.fmt_short()
                    );
                }

                // The receiver may have been dropped immediately if the caller is not interested
                // in awaiting the termination signal, so we ignore any potential error here.
                let _ = reply.send(());
            }
            ToTopicManager::Close { node_id, reply } => {
                if state.active_sync_set.remove(&node_id) {
                    debug!(
                        topic = state.topic.fmt_short(),
                        "removed node from active sync set: {}",
                        node_id.fmt_short()
                    );
                };

                for info in state.sessions.values() {
                    if info.remote == node_id {
                        state.live_sessions.close(info.session_id).await;
                    }
                }

                let _ = reply.send(());
            }
        }

        Ok(())
    }

    // Handle supervision events from sync session actors.
    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorTerminated(actor_cell, _, _) => {
                if let Some(info) = state.sessions.remove(&actor_cell.get_id()) {
                    state.live_sessions.close(info.session_id).await;

                    debug!(
                        session_id = info.session_id,
                        topic = state.topic.fmt_short(),
                        "sync session terminated"
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor_cell, err) => {
                if let Some(info) = state.sessions.remove(&actor_cell.get_id()) {
                    state.live_sessions.close(info.session_id).await;

                    warn!(
                        session_id = info.session_id,
                        topic = state.topic.fmt_short(),
                        "sync session failed: {err}"
                    );

                    // If this node was removed from the active sync set we skip retrying.
                    if !state.active_sync_set.contains(&info.remote) {
                        debug!(
                            remote = info.remote.fmt_short(),
                            topic = state.topic.fmt_short(),
                            "skip re-initiate sync: node no longer in active set"
                        );

                        return Ok(());
                    };

                    // Send a retry message to the actor after a 5 second delay, in the same mode as
                    // the failed session.
                    myself.send_after(RETRY_RATE, move || ToTopicManager::Retry {
                        node_id: info.remote,
                        live_mode: info.live_mode,
                    });
                }
            }
            _ => (),
        }

        Ok(())
    }
}

impl<S> TopicManager<S>
where
    S: SessionBuilder,
{
    /// Spawn a new sync session.
    ///
    /// Returns the actor ref and an option live-mode receiver which will be sent to the spawned
    /// session in an Initiate or Accept message.
    async fn spawn_session(
        myself: &ActorRef<ToTopicManager<S::Operation, S::Event>>,
        state: &mut TopicManagerState<S>,
        remote: &NodeId,
        live_mode: bool,
    ) -> Result<
        (
            ActorRef<SyncSessionMessage<S>>,
            Option<mpsc::Receiver<ToSync<S::Operation>>>,
        ),
        ActorProcessingErr,
    > {
        // Compute the next session id.
        let session_id = state.next_session_id;
        state.next_session_id += 1;

        // If this is a live-mode session we need to construct the live-mode channels and register
        // them so events can be forwarded between sessions.
        let live_rx = if live_mode {
            let (live_tx, live_rx) = mpsc::channel(LIVE_BUFFER);
            state.live_sessions.register(session_id, live_tx);
            Some(live_rx)
        } else {
            None
        };

        // Spawn the session actor.
        let session_state = SessionState {
            endpoint: state.endpoint.clone(),
            session_builder: state.session_builder.clone(),
            topic: state.topic,
            remote: *remote,
            session_id,
            parent_ref: myself.clone(),
        };

        let result = SyncSession::<S>::spawn_linked(
            None,
            session_state,
            myself.clone().into(),
            state.pool.clone(),
        )
        .await;

        // If spawning failed clean up the registered live-mode session channels.
        let (actor_ref, _) = match result {
            Ok(actor_ref) => actor_ref,
            Err(err) => {
                state.live_sessions.close(session_id).await;
                return Err(err.into());
            }
        };

        // Register this session on the actor.
        state.sessions.insert(
            actor_ref.get_id(),
            SessionInfo {
                session_id,
                remote: *remote,
                live_mode,
            },
        );

        Ok((actor_ref, live_rx))
    }
}
