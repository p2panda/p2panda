// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::time::Instant;

use iroh::protocol::ProtocolHandler;
use p2panda_discovery::DiscoveryResult;
use p2panda_discovery::address_book::AddressBookStore;
use ractor::concurrency::JoinHandle;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call, cast, registry};
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::discovery::session::{
    DiscoverySession, DiscoverySessionId, DiscoverySessionRole,
};
use crate::actors::discovery::walker::{DiscoveryWalker, ToDiscoveryWalker};
use crate::actors::discovery::{DISCOVERY_PROTOCOL_ID, DiscoveryActorName};
use crate::actors::iroh::register_protocol;
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::utils::to_public_key;

pub const DISCOVERY_MANAGER: &str = "net.discovery.manager";

pub enum ToDiscoveryManager<T> {
    /// Initiate a discovery session with the given node.
    ///
    /// A reference to the walker actor which initiated this session is kept, so the result of the
    /// session can be reported back to it.
    InitiateSession(NodeId, ActorRef<ToDiscoveryWalker<T>>),

    /// Accept a discovery session coming in from a remote node.
    AcceptSession(NodeId, iroh::endpoint::Connection),

    /// Received result from a successful discovery session.
    OnSuccess(DiscoverySessionId, DiscoveryResult<T, NodeId, NodeInfo>),

    /// Handle failed discovery session.
    OnFailure(DiscoverySessionId),

    /// Returns current metrics.
    Metrics(RpcReplyPort<DiscoveryMetrics>),
}

pub struct DiscoveryManagerState<S, T> {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    store: S,
    pool: ThreadLocalActorSpawner,
    next_session_id: DiscoverySessionId,
    sessions: HashMap<DiscoverySessionId, DiscoverySessionInfo>,
    walkers: HashMap<usize, WalkerInfo<T>>,
    metrics: DiscoveryMetrics,
}

impl<S, T> DiscoveryManagerState<S, T>
where
    T: Clone + Send + 'static,
{
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
        result: Option<DiscoveryResult<T, NodeId, NodeInfo>>,
    ) {
        let info = self
            .walkers
            .get_mut(&walker_id)
            .expect("walker with this id must exist");
        let _ = info
            .walker_ref
            .send_message(ToDiscoveryWalker::NextNode(result.clone()));
        info.last_result = result;
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
        let _ = info
            .walker_ref
            .send_message(ToDiscoveryWalker::NextNode(info.last_result.clone()));
    }
}

#[derive(Clone, Debug, Default)]
pub struct DiscoveryMetrics {
    /// Failed discovery sessions.
    failed_discovery_sessions: usize,

    /// Successful discovery sessions.
    successful_discovery_sessions: usize,

    /// Number of discovered transport infos which we're actually new for us.
    newly_learned_transport_infos: usize,
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

pub struct WalkerInfo<T> {
    #[allow(unused)]
    walker_id: usize,
    last_result: Option<DiscoveryResult<T, NodeId, NodeInfo>>,
    walker_ref: ActorRef<ToDiscoveryWalker<T>>,
    #[allow(unused)]
    handle: JoinHandle<()>,
}

#[derive(Debug)]
pub struct DiscoveryManager<S, T> {
    _marker: PhantomData<(S, T)>,
}

impl<S, T> Default for DiscoveryManager<S, T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S, T> ThreadLocalActor for DiscoveryManager<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Debug + Send + Sync + 'static,
    S::Error: StdError + Debug + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    type State = DiscoveryManagerState<S, T>;

    type Msg = ToDiscoveryManager<T>;

    type Arguments = (ApplicationArguments, S);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, store) = args;
        let actor_namespace = generate_actor_namespace(&args.public_key);
        let pool = ThreadLocalActorSpawner::new();

        // Accept incoming "discovery protocol" connection requests.
        register_protocol(
            DISCOVERY_PROTOCOL_ID,
            DiscoveryProtocolHandler {
                manager_ref: myself.clone(),
            },
            actor_namespace.clone(),
        )?;

        // Spawn random walkers. They automatically initiate discovery sessions.
        let mut walkers = HashMap::new();
        for walker_id in 0..args.discovery_config.random_walkers_count {
            let (walker_ref, handle) = DiscoveryWalker::spawn_linked(
                Some(DiscoveryActorName::new_walker(walker_id).to_string(&actor_namespace)),
                (args.clone(), store.clone(), myself.clone()),
                myself.clone().into(),
                pool.clone(),
            )
            .await?;

            // Start random walk, from now on it will run forever.
            walker_ref.send_message(ToDiscoveryWalker::NextNode(None))?;

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
            metrics: DiscoveryMetrics::default(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToDiscoveryManager::InitiateSession(node_id, walker_ref) => {
                // Sessions we've initiated ourselves are always connected to a particular walker.
                // Each walker can only ever run max. one discovery sessions at a time.
                let session_id = state.next_session_id();
                let walker_id = DiscoveryActorName::from_actor_ref(&walker_ref).walker_id();
                trace!(
                    session_id = %session_id,
                    walker_id = %walker_id,
                    node_id = %node_id,
                    "discovery session initiated"
                );

                let (_, handle) = DiscoverySession::spawn_linked(
                    Some(
                        DiscoveryActorName::new_session(session_id)
                            .to_string(&state.actor_namespace),
                    ),
                    (
                        generate_actor_namespace(&state.args.public_key),
                        session_id,
                        node_id,
                        state.store.clone(),
                        myself.clone(),
                        DiscoverySessionRole::Connect,
                    ),
                    myself.clone().into(),
                    state.pool.clone(),
                )
                .await?;

                state.sessions.insert(
                    session_id,
                    DiscoverySessionInfo::Initiated {
                        remote_node_id: node_id,
                        session_id,
                        walker_id,
                        started_at: Instant::now(),
                        handle,
                    },
                );
            }
            ToDiscoveryManager::AcceptSession(node_id, connection) => {
                // @TODO: Have a max. of concurrently running discovery sessions.
                let session_id = state.next_session_id();
                trace!(
                    session_id = %session_id,
                    node_id = %node_id,
                    "discovery session accepted"
                );

                let (_, handle) = DiscoverySession::spawn_linked(
                    Some(
                        DiscoveryActorName::new_accept_session(session_id)
                            .to_string(&state.actor_namespace),
                    ),
                    (
                        generate_actor_namespace(&state.args.public_key),
                        session_id,
                        node_id,
                        state.store.clone(),
                        myself.clone(),
                        DiscoverySessionRole::Accept { connection },
                    ),
                    myself.into(),
                    state.pool.clone(),
                )
                .await?;

                state.sessions.insert(
                    session_id,
                    DiscoverySessionInfo::Accepted {
                        remote_node_id: node_id,
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

                if let DiscoverySessionInfo::Initiated { walker_id, .. } = session_info {
                    // Continue random walk.
                    state.next_walk_step(walker_id, Some(discovery_result.clone()));
                }

                Self::insert_address_book(state, discovery_result).await;
            }
            ToDiscoveryManager::OnFailure(session_id) => {
                trace!(session_id = %session_id, "discovery session failed");
                state.metrics.failed_discovery_sessions += 1;
                let session_info = state
                    .sessions
                    .remove(&session_id)
                    .expect("session info to exist when session failed");

                if let DiscoverySessionInfo::Initiated { walker_id, .. } = session_info {
                    state.repeat_last_walk_step(walker_id);
                }
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
            SupervisionEvent::ActorFailed(actor, error) => {
                match DiscoveryActorName::from_actor_cell(&actor) {
                    DiscoveryActorName::Walker { walker_id } => {
                        // If an walker actor failed, we're expecting a bug in our system and
                        // escalate the error to a parent supervisor which will probably restart
                        // the whole thing.
                        return Err(ActorProcessingErr::from(format!(
                            "walker actor {walker_id} failed with error: {error}"
                        )));
                    }
                    DiscoveryActorName::Session { session_id }
                    | DiscoveryActorName::AcceptedSession { session_id } => {
                        myself.send_message(ToDiscoveryManager::OnFailure(session_id))?;
                    }
                }
            }
            _ => (),
        }

        Ok(())
    }
}

impl<S, T> DiscoveryManager<S, T>
where
    T: Send + 'static,
{
    async fn insert_address_book(
        state: &mut DiscoveryManagerState<S, T>,
        discovery_result: DiscoveryResult<T, NodeId, NodeInfo>,
    ) {
        // Ignore missing address book actor or receive errors. This means the system is shutting
        // down.
        let address_book_ref = {
            let Some(actor) =
                registry::where_is(with_namespace(ADDRESS_BOOK, &state.actor_namespace))
            else {
                return;
            };
            ActorRef::<ToAddressBook<T>>::from(actor)
        };

        // Populate address book with hopefully new transport info.
        for (node_id, transport_info) in &discovery_result.node_transport_infos {
            let Ok(result) = call!(
                address_book_ref,
                ToAddressBook::InsertTransportInfo,
                *node_id,
                transport_info.clone()
            ) else {
                return;
            };

            match result {
                Ok(is_new_info) => {
                    if is_new_info {
                        state.metrics.newly_learned_transport_infos += 1;
                    }
                }
                Err(_) => {
                    // If this insertion fails we know that some of the given information was
                    // invalid (eg. wrong signature) and we stop here as we can't trust this node.
                    //
                    // @TODO: Later we want to "rate" this node as misbehaving.
                    return;
                }
            }
        }

        // Set stream topics into address book for this node.
        let _ = cast!(
            address_book_ref,
            ToAddressBook::SetTopics(
                discovery_result.remote_node_id,
                discovery_result.node_topics.into_iter().collect::<Vec<T>>()
            )
        );

        // Set ephemeral stream topics into address book for this node.
        let _ = cast!(
            address_book_ref,
            ToAddressBook::SetTopicIds(
                discovery_result.remote_node_id,
                discovery_result
                    .node_topic_ids
                    .into_iter()
                    .collect::<Vec<TopicId>>()
            )
        );
    }
}

#[derive(Debug)]
struct DiscoveryProtocolHandler<T> {
    manager_ref: ActorRef<ToDiscoveryManager<T>>,
}

impl<T> ProtocolHandler for DiscoveryProtocolHandler<T>
where
    T: Debug + Send + 'static,
{
    async fn accept(
        &self,
        connection: iroh::endpoint::Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        self.manager_ref
            .send_message(ToDiscoveryManager::<T>::AcceptSession(
                to_public_key(connection.remote_id()),
                connection,
            ))
            .map_err(|err| iroh::protocol::AcceptError::from_err(err))
    }
}
