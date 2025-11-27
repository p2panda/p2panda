// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Instant;

use iroh::protocol::ProtocolHandler;
use p2panda_discovery::DiscoveryResult;
use p2panda_discovery::address_book::AddressBookStore;
use ractor::concurrency::JoinHandle;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call, cast, registry};
use tokio::sync::Notify;
use tracing::{debug, warn};

use crate::actors::address_book::{
    ADDRESS_BOOK, ToAddressBook, watch_node_info, watch_node_topics,
};
use crate::actors::discovery::session::{
    DiscoverySession, DiscoverySessionArguments, DiscoverySessionId, DiscoverySessionRole,
};
use crate::actors::discovery::walker::{DiscoveryWalker, ToDiscoveryWalker, WalkFromHere};
use crate::actors::discovery::{DISCOVERY_PROTOCOL_ID, DiscoveryActorName};
use crate::actors::iroh::register_protocol;
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::utils::{ShortFormat, to_public_key};

pub const DISCOVERY_MANAGER: &str = "net.discovery.manager";

pub enum ToDiscoveryManager {
    /// Initiate a discovery session with the given node.
    ///
    /// A reference to the walker actor which initiated this session is kept, so the result of the
    /// session can be reported back to it.
    InitiateSession(NodeId, ActorRef<ToDiscoveryWalker>),

    /// Accept a discovery session coming in from a remote node.
    AcceptSession(NodeId, iroh::endpoint::Connection),

    /// Received result from a successful discovery session.
    OnSuccess(DiscoverySessionId, DiscoveryResult<NodeId, NodeInfo>),

    /// Handle failed discovery session.
    OnFailure(
        DiscoverySessionId,
        Box<dyn StdError + Send + Sync + 'static>,
    ),

    /// Reset backoff logic of all walkers and make them start from the bootstrap set again. This
    /// will allow them to do their work faster and can be used to improve the user experience in
    /// moments where the application needs discovery.
    ResetWalkers,

    /// Returns current metrics.
    #[allow(unused)]
    Metrics(RpcReplyPort<DiscoveryMetrics>),
}

pub struct DiscoveryManagerState<S> {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    store: S,
    pool: ThreadLocalActorSpawner,
    next_session_id: DiscoverySessionId,
    sessions: HashMap<DiscoverySessionId, DiscoverySessionInfo>,
    walkers: HashMap<usize, WalkerInfo>,
    walkers_reset: Arc<Notify>,
    watch_handle: Option<JoinHandle<()>>,
    metrics: DiscoveryMetrics,
}

impl<S> DiscoveryManagerState<S> {
    /// Return next id for a discovery session.
    pub fn next_session_id(&mut self) -> DiscoverySessionId {
        let session_id = self.next_session_id;
        self.next_session_id += 1;
        session_id
    }

    /// Continue random walk using the result of the last successful session.
    pub fn next_walk_step(
        &mut self,
        walker_id: usize,
        discovery_result: DiscoveryResult<NodeId, NodeInfo>,
        newly_learned_transport_infos: usize,
    ) {
        let info = self
            .walkers
            .get_mut(&walker_id)
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
    pub fn repeat_last_walk_step(&self, walker_id: usize) {
        let info = self
            .walkers
            .get(&walker_id)
            .expect("walker with this id must exist");
        let _ = info.walker_ref.send_message(ToDiscoveryWalker::NextNode(
            WalkFromHere::FailedSession {
                last_successful: info.last_result.clone(),
            },
        ));
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

#[allow(unused)]
pub enum DiscoverySessionInfo {
    Initiated {
        remote_node_id: NodeId,
        session_id: DiscoverySessionId,
        walker_id: usize,
        started_at: Instant,
        handle: JoinHandle<()>,
    },
    Accepted {
        remote_node_id: NodeId,
        session_id: DiscoverySessionId,
        started_at: Instant,
        handle: JoinHandle<()>,
    },
}

impl DiscoverySessionInfo {
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

#[derive(Debug)]
pub struct DiscoveryManager<S> {
    _marker: PhantomData<S>,
}

impl<S> Default for DiscoveryManager<S> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S> ThreadLocalActor for DiscoveryManager<S>
where
    S: AddressBookStore<NodeId, NodeInfo> + Clone + Debug + Send + Sync + 'static,
    S::Error: StdError + Debug + Send + Sync + 'static,
{
    type State = DiscoveryManagerState<S>;

    type Msg = ToDiscoveryManager;

    type Arguments = (ApplicationArguments, S);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, store) = args;
        let actor_namespace = generate_actor_namespace(&args.public_key);
        let pool = ThreadLocalActorSpawner::new();

        // Spawn random walkers. They automatically initiate discovery sessions.
        let mut walkers = HashMap::new();
        let walkers_reset = Arc::new(Notify::new());
        for walker_id in 0..args.discovery_config.random_walkers_count {
            let (walker_ref, handle) = DiscoveryWalker::spawn_linked(
                Some(DiscoveryActorName::new_walker(walker_id).to_string(&actor_namespace)),
                (
                    args.clone(),
                    store.clone(),
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
                walker_id,
                WalkerInfo {
                    walker_id,
                    last_result: None,
                    walker_ref,
                    handle,
                },
            );
        }

        Ok(DiscoveryManagerState {
            actor_namespace,
            args,
            store,
            pool,
            next_session_id: 0,
            sessions: HashMap::new(),
            walkers,
            walkers_reset,
            watch_handle: None,
            metrics: DiscoveryMetrics::default(),
        })
    }

    async fn post_start(
        &self,
        myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Accept incoming "discovery protocol" connection requests.
        register_protocol(
            DISCOVERY_PROTOCOL_ID,
            DiscoveryProtocolHandler {
                manager_ref: myself.clone(),
            },
            state.actor_namespace.clone(),
        )?;

        // Watch for topic changes of our node (to find out if user subscribed to a new
        // topic) and watch for transport info changing. If yes, we want to reset the
        // backoff logic of the walkers to allow finding nodes "faster" for this newly
        // subscribed topic or networking setup.
        let mut topics_rx =
            watch_node_topics(state.actor_namespace.clone(), state.args.public_key, true).await?;

        let mut node_info_rx =
            watch_node_info(state.actor_namespace.clone(), state.args.public_key, true).await?;

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

                        debug!("detected new topic subscription, reset walkers");
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

                        debug!("detected our transport info changing, reset walkers");
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

        Ok(())
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
            ToDiscoveryManager::InitiateSession(remote_node_id, walker_ref) => {
                // Sessions we've initiated ourselves are always connected to a particular walker.
                // Each walker can only ever run max. one discovery sessions at a time.
                let session_id = state.next_session_id();
                let walker_id = DiscoveryActorName::from_actor_ref(&walker_ref).walker_id();

                let (_, handle) = DiscoverySession::spawn_linked(
                    Some(
                        DiscoveryActorName::new_session(session_id)
                            .to_string(&state.actor_namespace),
                    ),
                    DiscoverySessionArguments {
                        my_node_id: state.args.public_key,
                        session_id,
                        remote_node_id,
                        store: state.store.clone(),
                        manager_ref: myself.clone(),
                        args: DiscoverySessionRole::Connect,
                    },
                    myself.clone().into(),
                    state.pool.clone(),
                )
                .await?;

                state.sessions.insert(
                    session_id,
                    DiscoverySessionInfo::Initiated {
                        remote_node_id,
                        session_id,
                        walker_id,
                        started_at: Instant::now(),
                        handle,
                    },
                );
            }
            ToDiscoveryManager::AcceptSession(remote_node_id, connection) => {
                // @TODO: Have a max. of concurrently running discovery sessions.
                let session_id = state.next_session_id();

                let (_, handle) = DiscoverySession::spawn_linked(
                    Some(
                        DiscoveryActorName::new_accept_session(session_id)
                            .to_string(&state.actor_namespace),
                    ),
                    DiscoverySessionArguments {
                        my_node_id: state.args.public_key,
                        session_id,
                        remote_node_id,
                        store: state.store.clone(),
                        manager_ref: myself.clone(),
                        args: DiscoverySessionRole::Accept { connection },
                    },
                    myself.into(),
                    state.pool.clone(),
                )
                .await?;

                state.sessions.insert(
                    session_id,
                    DiscoverySessionInfo::Accepted {
                        remote_node_id,
                        session_id,
                        started_at: Instant::now(),
                        handle,
                    },
                );
            }
            ToDiscoveryManager::OnSuccess(session_id, discovery_result) => {
                state.metrics.successful_discovery_sessions += 1;
                let session_info = state
                    .sessions
                    .remove(&session_id)
                    .expect("session info to exist when it successfully ended");
                debug!(
                    %session_id,
                    node_id = session_info.remote_node_id().fmt_short(),
                    duration_ms = session_info.started_at().elapsed().as_millis(),
                    transport_infos = %discovery_result.node_transport_infos.len(),
                    sync_topics = %discovery_result.sync_topics.len(),
                    ephemeral_messaging_topics = %discovery_result.ephemeral_messaging_topics.len(),
                    "successful discovery session"
                );

                let newly_learned_transport_infos =
                    Self::insert_address_book(state, &discovery_result).await;
                state.metrics.newly_learned_transport_infos += newly_learned_transport_infos;

                if let DiscoverySessionInfo::Initiated { walker_id, .. } = session_info {
                    // Continue random walk.
                    state.next_walk_step(
                        walker_id,
                        discovery_result.clone(),
                        newly_learned_transport_infos,
                    );
                }
            }
            ToDiscoveryManager::OnFailure(session_id, err) => {
                state.metrics.failed_discovery_sessions += 1;
                let session_info = state
                    .sessions
                    .remove(&session_id)
                    .expect("session info to exist when session failed");
                warn!(
                    %session_id,
                    node_id = session_info.remote_node_id().fmt_short(),
                    duration_ms = session_info.started_at().elapsed().as_millis(),
                    "failed discovery session: {err:#}"
                );

                if let DiscoverySessionInfo::Initiated { walker_id, .. } = session_info {
                    // Continue random walk.
                    state.repeat_last_walk_step(walker_id);
                }
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
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorTerminated(actor, _, _) => {
                match DiscoveryActorName::from_actor_cell(&actor) {
                    DiscoveryActorName::Walker { walker_id } => {
                        // Shutting down a walker means we're shutting down the discovery system,
                        // including this manager.
                        myself.stop(Some(format!("walker {walker_id} shutting down")));
                    }
                    DiscoveryActorName::Session { .. }
                    | DiscoveryActorName::AcceptedSession { .. } => {
                        // When a discovery session terminates successfully we will deal with it in
                        // "FinishSession" instead.
                    }
                }
            }
            SupervisionEvent::ActorFailed(actor, err) => {
                match DiscoveryActorName::from_actor_cell(&actor) {
                    DiscoveryActorName::Walker { walker_id } => {
                        // If an walker actor failed, we're expecting a bug in our system and
                        // escalate the error to a parent supervisor which will probably restart
                        // the whole thing.
                        return Err(ActorProcessingErr::from(format!(
                            "walker actor {walker_id} failed with error: {err}"
                        )));
                    }
                    DiscoveryActorName::Session { session_id }
                    | DiscoveryActorName::AcceptedSession { session_id } => {
                        myself.send_message(ToDiscoveryManager::OnFailure(session_id, err))?;
                    }
                }
            }
            _ => (),
        }

        Ok(())
    }
}

impl<S> DiscoveryManager<S> {
    async fn insert_address_book(
        state: &mut DiscoveryManagerState<S>,
        discovery_result: &DiscoveryResult<NodeId, NodeInfo>,
    ) -> usize {
        // Ignore missing address book actor or receive errors. This means the system is shutting
        // down.
        let address_book_ref = {
            let Some(actor) =
                registry::where_is(with_namespace(ADDRESS_BOOK, &state.actor_namespace))
            else {
                return 0;
            };
            ActorRef::<ToAddressBook>::from(actor)
        };

        // Populate address book with hopefully new transport info.
        let mut newly_learned_transport_infos = 0;
        for (node_id, transport_info) in &discovery_result.node_transport_infos {
            let Ok(result) = call!(
                address_book_ref,
                ToAddressBook::InsertTransportInfo,
                *node_id,
                transport_info.clone().into()
            ) else {
                return 0;
            };

            match result {
                Ok(is_new_info) => {
                    if is_new_info {
                        newly_learned_transport_infos += 1;
                    }
                }
                Err(_) => {
                    // If this insertion fails we know that some of the given information was
                    // invalid (eg. wrong signature) and we stop here as we can't trust this node.
                    //
                    // @TODO: Later we want to "rate" this node as misbehaving.
                    return newly_learned_transport_infos;
                }
            }
        }

        // Set stream topics into address book for this node.
        let _ = cast!(
            address_book_ref,
            ToAddressBook::SetSyncTopics(
                discovery_result.remote_node_id,
                discovery_result.sync_topics.clone(),
            )
        );

        // Set ephemeral stream topics into address book for this node.
        let _ = cast!(
            address_book_ref,
            ToAddressBook::SetEphemeralMessagingTopics(
                discovery_result.remote_node_id,
                discovery_result.ephemeral_messaging_topics.clone()
            )
        );

        newly_learned_transport_infos
    }
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
