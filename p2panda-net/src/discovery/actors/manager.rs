// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iroh::endpoint::TransportConfig;
use iroh::protocol::ProtocolHandler;
use p2panda_discovery::DiscoveryResult;
use p2panda_discovery::address_book::NodeInfo as _;
use ractor::concurrency::JoinHandle;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorId, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use rand_chacha::ChaCha20Rng;
use tokio::sync::{Notify, broadcast};
use tracing::{debug, warn};

use crate::NodeId;
use crate::address_book::{AddressBook, AddressBookError};
use crate::addrs::NodeInfo;
use crate::discovery::DiscoveryConfig;
use crate::discovery::actors::DISCOVERY_PROTOCOL_ID;
use crate::discovery::actors::session::{
    DiscoverySession, DiscoverySessionArguments, DiscoverySessionId, DiscoverySessionRole,
    ToDiscoverySession,
};
use crate::discovery::actors::walker::{DiscoveryWalker, ToDiscoveryWalker, WalkFromHere};
use crate::discovery::events::{DiscoveryEvent, SessionRole};
use crate::iroh_endpoint::{Endpoint, to_public_key};
use crate::utils::ShortFormat;

/// Maximum duration of inactivity to accept before timing out the connection.
pub const MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(3);

pub enum ToDiscoveryManager {
    /// Accept incoming "discovery protocol" connection requests.
    Initiate,

    /// Initiate a discovery session with the given node.
    ///
    /// A reference to the walker actor which initiated this session is kept, so the result of the
    /// session can be reported back to it.
    InitiateSession(NodeId, ActorRef<ToDiscoveryWalker>),

    /// Accept a discovery session coming in from a remote node.
    AcceptSession(NodeId, iroh::endpoint::Connection),

    /// Received result from a successful discovery session.
    OnSuccess(
        ActorRef<ToDiscoverySession>,
        DiscoveryResult<NodeId, NodeInfo>,
    ),

    /// Handle failed discovery session.
    OnFailure(
        ActorRef<ToDiscoverySession>,
        Box<dyn StdError + Send + Sync + 'static>,
    ),

    /// Reset backoff logic of all walkers and make them start from the bootstrap set again. This
    /// will allow them to do their work faster and can be used to improve the user experience in
    /// moments where the application needs discovery.
    ResetWalkers,

    /// Subscribe to system events.
    Events(RpcReplyPort<broadcast::Receiver<DiscoveryEvent>>),

    /// Returns current metrics.
    Metrics(RpcReplyPort<DiscoveryMetrics>),
}

pub struct DiscoveryManagerState {
    my_node_id: NodeId,
    address_book: AddressBook,
    endpoint: Endpoint,
    pool: ThreadLocalActorSpawner,
    next_session_id: DiscoverySessionId,
    sessions: HashMap<ActorId, DiscoverySessionInfo>,
    walkers: HashMap<ActorId, WalkerInfo>,
    walkers_reset: Arc<Notify>,
    watch_handle: Option<JoinHandle<()>>,
    events_tx: broadcast::Sender<DiscoveryEvent>,
    transport_config: Arc<TransportConfig>,
    metrics: DiscoveryMetrics,
}

impl DiscoveryManagerState {
    /// Return next id for a discovery session.
    pub fn next_session_id(&mut self) -> DiscoverySessionId {
        let session_id = self.next_session_id;
        self.next_session_id += 1;
        session_id
    }

    /// Continue random walk using the result of the last successful session.
    pub fn next_walk_step(
        &mut self,
        walker_ref: ActorRef<ToDiscoveryWalker>,
        discovery_result: DiscoveryResult<NodeId, NodeInfo>,
        newly_learned_transport_infos: usize,
    ) {
        let info = self
            .walkers
            .get_mut(&walker_ref.get_id())
            .expect("walker with this id must exist");
        let _ =
            info.walker_ref
                .send_message(ToDiscoveryWalker::NextNode(WalkFromHere::LastSession {
                    discovery_result: discovery_result.clone(),
                    newly_learned_transport_infos,
                }));
        info.last_result = Some(discovery_result);
    }

    /// Continue random walk by re-using the previous result which hopefully came from a successful
    /// session.
    ///
    /// The walker will then attempt continuing with other nodes. If there is none left it will
    /// automatically start from scratch.
    pub fn repeat_last_walk_step(&self, walker_ref: ActorRef<ToDiscoveryWalker>) {
        let info = self
            .walkers
            .get(&walker_ref.get_id())
            .expect("walker with this id must exist");
        let _ = info.walker_ref.send_message(ToDiscoveryWalker::NextNode(
            WalkFromHere::FailedSession {
                last_successful: info.last_result.clone(),
            },
        ));
    }

    /// Returns true if the given node is stale.
    pub async fn is_stale(&self, remote_node_id: NodeId) -> bool {
        let Ok(Some(node_info)) = self.address_book.node_info(remote_node_id).await else {
            return false;
        };

        node_info.is_stale()
    }
}

#[derive(Clone, Debug, Default)]
pub struct DiscoveryMetrics {
    /// Failed discovery sessions.
    pub failed_discovery_sessions: usize,

    /// Successful discovery sessions.
    pub successful_discovery_sessions: usize,

    /// Number of discovered transport infos which were actually new for us.
    pub newly_learned_transport_infos: usize,
}

pub enum DiscoverySessionInfo {
    Initiated {
        remote_node_id: NodeId,
        session_id: DiscoverySessionId,
        walker_ref: ActorRef<ToDiscoveryWalker>,
        session_ref: ActorRef<ToDiscoverySession>,
        started_at: Instant,
        #[allow(unused)]
        handle: JoinHandle<()>,
    },
    #[allow(unused)]
    Accepted {
        remote_node_id: NodeId,
        session_id: DiscoverySessionId,
        session_ref: ActorRef<ToDiscoverySession>,
        started_at: Instant,
        handle: JoinHandle<()>,
    },
}

impl DiscoverySessionInfo {
    pub fn session_id(&self) -> DiscoverySessionId {
        match self {
            DiscoverySessionInfo::Initiated { session_id, .. } => *session_id,
            DiscoverySessionInfo::Accepted { session_id, .. } => *session_id,
        }
    }

    pub fn session_ref(&self) -> ActorRef<ToDiscoverySession> {
        match self {
            DiscoverySessionInfo::Initiated { session_ref, .. } => session_ref.clone(),
            DiscoverySessionInfo::Accepted { session_ref, .. } => session_ref.clone(),
        }
    }

    pub fn remote_node_id(&self) -> NodeId {
        match self {
            DiscoverySessionInfo::Initiated { remote_node_id, .. } => *remote_node_id,
            DiscoverySessionInfo::Accepted { remote_node_id, .. } => *remote_node_id,
        }
    }

    pub fn started_at(&self) -> &Instant {
        match self {
            DiscoverySessionInfo::Initiated { started_at, .. } => started_at,
            DiscoverySessionInfo::Accepted { started_at, .. } => started_at,
        }
    }
}

pub struct WalkerInfo {
    #[allow(unused)]
    walker_id: usize,
    last_result: Option<DiscoveryResult<NodeId, NodeInfo>>,
    walker_ref: ActorRef<ToDiscoveryWalker>,
    #[allow(unused)]
    handle: JoinHandle<()>,
}

#[derive(Debug, Default)]
pub struct DiscoveryManager;

impl ThreadLocalActor for DiscoveryManager {
    type State = DiscoveryManagerState;

    type Msg = ToDiscoveryManager;

    type Arguments = (DiscoveryConfig, ChaCha20Rng, AddressBook, Endpoint);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (config, rng, address_book, endpoint) = args;
        let pool = ThreadLocalActorSpawner::new();
        let my_node_id = endpoint.node_id();

        // Spawn random walkers. They automatically initiate discovery sessions.
        let mut walkers = HashMap::new();
        let walkers_reset = Arc::new(Notify::new());
        for walker_id in 0..config.random_walkers_count {
            let (walker_ref, handle) = DiscoveryWalker::spawn_linked(
                None,
                (
                    my_node_id,
                    config.clone(),
                    address_book.store().await?,
                    rng.clone(),
                    walkers_reset.clone(),
                    myself.clone(),
                ),
                myself.clone().into(),
                pool.clone(),
            )
            .await?;

            // Start random walk from bootstrap nodes (if available), from now on it will run
            // forever.
            walker_ref.send_message(ToDiscoveryWalker::NextNode(WalkFromHere::Bootstrap))?;

            walkers.insert(
                walker_ref.get_id(),
                WalkerInfo {
                    walker_id,
                    last_result: None,
                    walker_ref,
                    handle,
                },
            );
        }

        // Custom QUIC transport parameters for discovery protocol. We don't want to wait too long
        // for unreachable nodes. QUIC should fastly tell us about a timeout which will mark this
        // node as "stale".
        let mut transport_config = TransportConfig::default();
        transport_config.max_idle_timeout(Some(
            MAX_IDLE_TIMEOUT.try_into().expect("correct max idle value"),
        ));

        // Invoke the handler to register the discovery protocol and do other setups.
        let _ = myself.cast(ToDiscoveryManager::Initiate);

        let (events_tx, _) = broadcast::channel(64);

        Ok(DiscoveryManagerState {
            my_node_id,
            address_book,
            endpoint,
            pool,
            next_session_id: 0,
            sessions: HashMap::new(),
            walkers,
            walkers_reset,
            watch_handle: None,
            events_tx,
            transport_config: Arc::new(transport_config),
            metrics: DiscoveryMetrics::default(),
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(handle) = &state.watch_handle {
            handle.abort();
        }

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToDiscoveryManager::Initiate => {
                // Accept incoming "discovery protocol" connection requests.
                state
                    .endpoint
                    .accept(
                        DISCOVERY_PROTOCOL_ID,
                        DiscoveryProtocolHandler {
                            manager_ref: myself.clone(),
                        },
                    )
                    .await?;

                // Watch for topic changes of our node (to find out if user subscribed to a new
                // topic) and watch for transport info changing. If yes, we want to reset the
                // backoff logic of the walkers to allow finding nodes "faster" for this newly
                // subscribed topic or networking setup.
                let mut topics_rx = state
                    .address_book
                    .watch_node_topics(state.my_node_id, true)
                    .await?;

                let mut node_info_rx = state
                    .address_book
                    .watch_node_info(state.my_node_id, true)
                    .await?;

                let handle = tokio::task::spawn(async move {
                    loop {
                        tokio::select! {
                            Some(event) = topics_rx.recv() => {
                                // Reset walkers if topics have been _added_ to our set, ignore
                                // removed topics.
                                let difference = event.difference.unwrap_or_default();
                                if difference.is_empty() || !difference.is_subset(&event.value) {
                                    continue;
                                }
                            }
                            Some(event) = node_info_rx.recv() => {
                                // Reset walkers if new transport info was set, ignore if we're
                                // offline (and transport info is `None`).
                                match event.value {
                                    Some(node_info) => if node_info.transports.is_none() {
                                        continue;
                                    },
                                    None => continue,
                                }
                            }
                            else => {
                                // When a watcher closed it's sender part, we know the address book
                                // actor shut down and assume the process is closing for good.
                                break;
                            }
                        }

                        if myself
                            .send_message(ToDiscoveryManager::ResetWalkers)
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                state.watch_handle = Some(handle);
            }
            ToDiscoveryManager::InitiateSession(remote_node_id, walker_ref) => {
                // Check if this node became stale in the meantime and cancel session if so.
                if state.is_stale(remote_node_id).await {
                    state.repeat_last_walk_step(walker_ref);
                    return Ok(());
                }

                let session_id = state.next_session_id();

                let (session_ref, handle) = DiscoverySession::spawn_linked(
                    None,
                    DiscoverySessionArguments {
                        my_node_id: state.my_node_id,
                        remote_node_id,
                        store: state.address_book.store().await?,
                        endpoint: state.endpoint.clone(),
                        manager_ref: myself.clone(),
                        transport_config: state.transport_config.clone(),
                        args: DiscoverySessionRole::Connect,
                    },
                    myself.clone().into(),
                    state.pool.clone(),
                )
                .await?;

                state.sessions.insert(
                    session_ref.get_id(),
                    DiscoverySessionInfo::Initiated {
                        remote_node_id,
                        session_id,
                        session_ref,
                        walker_ref,
                        started_at: Instant::now(),
                        handle,
                    },
                );

                // Inform subscribers about this discovery "system" event.
                let _ = state.events_tx.send(DiscoveryEvent::SessionStarted {
                    role: SessionRole::Initiated,
                    remote_node_id,
                });
            }
            ToDiscoveryManager::AcceptSession(remote_node_id, connection) => {
                // TODO: Have a max. of concurrently running discovery sessions.
                let session_id = state.next_session_id();

                let (session_ref, handle) = DiscoverySession::spawn_linked(
                    None,
                    DiscoverySessionArguments {
                        my_node_id: state.my_node_id,
                        remote_node_id,
                        store: state.address_book.store().await?,
                        endpoint: state.endpoint.clone(),
                        manager_ref: myself.clone(),
                        transport_config: state.transport_config.clone(),
                        args: DiscoverySessionRole::Accept { connection },
                    },
                    myself.into(),
                    state.pool.clone(),
                )
                .await?;

                state.sessions.insert(
                    session_ref.get_id(),
                    DiscoverySessionInfo::Accepted {
                        remote_node_id,
                        session_id,
                        session_ref,
                        started_at: Instant::now(),
                        handle,
                    },
                );

                // Inform subscribers about this discovery "system" event.
                let _ = state.events_tx.send(DiscoveryEvent::SessionStarted {
                    role: SessionRole::Accepted,
                    remote_node_id,
                });
            }
            ToDiscoveryManager::OnSuccess(session_ref, discovery_result) => {
                state.metrics.successful_discovery_sessions += 1;

                let session_info = state
                    .sessions
                    .remove(&session_ref.get_id())
                    .expect("session info to exist when it successfully ended");
                let duration = session_info.started_at().elapsed();

                debug!(
                    session_id = &session_info.session_id(),
                    node_id = session_info.remote_node_id().fmt_short(),
                    duration_ms = duration.as_millis(),
                    transport_infos = %discovery_result.transport_infos.len(),
                    topics = %discovery_result.topics.len(),
                    "successful discovery session"
                );

                let newly_learned_transport_infos =
                    insert_address_book(state, discovery_result.clone()).await;
                state.metrics.newly_learned_transport_infos += newly_learned_transport_infos;

                if let DiscoverySessionInfo::Initiated { ref walker_ref, .. } = session_info {
                    // Continue random walk.
                    state.next_walk_step(
                        walker_ref.clone(),
                        discovery_result.clone(),
                        newly_learned_transport_infos,
                    );
                }

                // Inform subscribers about this discovery "system" event.
                match session_info {
                    DiscoverySessionInfo::Initiated { remote_node_id, .. } => {
                        let _ = state.events_tx.send(DiscoveryEvent::SessionEnded {
                            role: SessionRole::Initiated,
                            remote_node_id,
                            result: discovery_result,
                            duration,
                        });
                    }
                    DiscoverySessionInfo::Accepted { remote_node_id, .. } => {
                        let _ = state.events_tx.send(DiscoveryEvent::SessionEnded {
                            role: SessionRole::Accepted,
                            remote_node_id,
                            result: discovery_result,
                            duration,
                        });
                    }
                }
            }
            ToDiscoveryManager::OnFailure(session_ref, err) => {
                state.metrics.failed_discovery_sessions += 1;

                let session_info = state
                    .sessions
                    .remove(&session_ref.get_id())
                    .expect("session info to exist when session failed");
                let duration = session_info.started_at().elapsed();

                warn!(
                    session_id = %session_info.session_id(),
                    node_id = session_info.remote_node_id().fmt_short(),
                    duration_ms = duration.as_millis(),
                    "failed discovery session: {err:#}"
                );

                if let DiscoverySessionInfo::Initiated { ref walker_ref, .. } = session_info {
                    // Continue random walk.
                    state.repeat_last_walk_step(walker_ref.clone());
                }

                // Inform subscribers about this discovery "system" event.
                match session_info {
                    DiscoverySessionInfo::Initiated { remote_node_id, .. } => {
                        let _ = state.events_tx.send(DiscoveryEvent::SessionFailed {
                            role: SessionRole::Initiated,
                            remote_node_id,
                            duration,
                            reason: err.to_string(),
                        });
                    }
                    DiscoverySessionInfo::Accepted { remote_node_id, .. } => {
                        let _ = state.events_tx.send(DiscoveryEvent::SessionFailed {
                            role: SessionRole::Accepted,
                            remote_node_id,
                            duration,
                            reason: err.to_string(),
                        });
                    }
                }
            }
            ToDiscoveryManager::Events(reply) => {
                let _ = reply.send(state.events_tx.subscribe());
            }
            ToDiscoveryManager::ResetWalkers => {
                state.walkers_reset.notify_waiters();
            }
            ToDiscoveryManager::Metrics(reply) => {
                let _ = reply.send(state.metrics.clone());
            }
        }
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorTerminated(actor_cell, _, _) => {
                if state.walkers.contains_key(&actor_cell.get_id()) {
                    // Shutting down a walker means we're shutting down the discovery system,
                    // including this manager.
                    myself.stop(Some("walker shutting down".into()));
                } else {
                    // When a discovery session terminates successfully we will deal with it in
                    // "FinishSession" instead.
                }
            }
            SupervisionEvent::ActorFailed(actor_cell, err) => {
                if state.walkers.contains_key(&actor_cell.get_id()) {
                    // If an walker actor failed, we're expecting a bug in our system and escalate
                    // the error to a parent supervisor which will probably restart the whole
                    // thing.
                    return Err(ActorProcessingErr::from(format!(
                        "walker actor failed with error: {err}"
                    )));
                }

                if let Some(info) = state.sessions.get(&actor_cell.get_id()) {
                    myself.send_message(ToDiscoveryManager::OnFailure(info.session_ref(), err))?;
                }
            }
            _ => (),
        }

        Ok(())
    }
}

/// Populates the address book with results from discovery session and returns number of newly
/// learned transport infos.
async fn insert_address_book(
    state: &mut DiscoveryManagerState,
    discovery_result: DiscoveryResult<NodeId, NodeInfo>,
) -> usize {
    // Populate address book with hopefully new transport info.
    let mut newly_learned_transport_infos = 0;
    for (node_id, transport_info) in &discovery_result.transport_infos {
        match state
            .address_book
            .insert_transport_info(*node_id, transport_info.clone().into())
            .await
        {
            Ok(is_new_info) => {
                if is_new_info {
                    newly_learned_transport_infos += 1;
                }
            }
            Err(AddressBookError::NodeInfo(_)) => {
                // If this insertion fails we know that some of the given information was
                // invalid (eg. wrong signature) and we stop here as we can't trust this node.
                //
                // TODO: Later we want to "report" this node as misbehaving.
                return newly_learned_transport_infos;
            }
            Err(_) => {
                return 0;
            }
        }
    }

    // Remember topics for this node.
    let _ = state
        .address_book
        .set_topics(discovery_result.remote_node_id, discovery_result.topics)
        .await;

    newly_learned_transport_infos
}

#[derive(Debug)]
struct DiscoveryProtocolHandler {
    manager_ref: ActorRef<ToDiscoveryManager>,
}

impl ProtocolHandler for DiscoveryProtocolHandler {
    async fn accept(
        &self,
        connection: iroh::endpoint::Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        self.manager_ref
            .send_message(ToDiscoveryManager::AcceptSession(
                to_public_key(connection.remote_id()),
                connection,
            ))
            .map_err(|err| iroh::protocol::AcceptError::from_err(err))
    }
}
