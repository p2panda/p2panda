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
use ractor::{ActorProcessingErr, ActorRef, SupervisionEvent, call, cast, registry};
use serde::{Deserialize, Serialize};

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::discovery::session::{
    DiscoverySession, DiscoverySessionArguments, DiscoverySessionId,
};
use crate::actors::discovery::walker::{DiscoveryWalker, ToDiscoveryWalker};
use crate::actors::discovery::{DISCOVERY_PROTOCOL_ID, DiscoveryActorName};
use crate::actors::iroh::register_protocol;
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::utils::to_public_key;

pub const DISCOVERY_MANAGER: &str = "net.discovery.manager";

#[allow(clippy::enum_variant_names)]
pub enum ToDiscoveryManager<T> {
    /// Initiate a discovery session with the given node.
    ///
    /// A reference to the walker actor which initiated this session is kept, so the result of the
    /// session can be reported back to it.
    StartSession(NodeId, ActorRef<ToDiscoveryWalker<T>>),

    /// Accept a discovery session with the given node.
    AcceptSession(NodeId, iroh::endpoint::Connection),

    /// Received result from a successful discovery session.
    FinishSession(DiscoverySessionId, DiscoveryResult<T, NodeId, NodeInfo>),
}

pub struct DiscoveryManagerState<S, T> {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    store: S,
    pool: ThreadLocalActorSpawner,
    next_session_id: DiscoverySessionId,
    sessions: HashMap<DiscoverySessionId, DiscoverySessionInfo>,
    metrics: DiscoveryMetrics,
    _marker: PhantomData<T>,
}

impl<S, T> DiscoveryManagerState<S, T> {
    pub fn next_session_id(&mut self) -> DiscoverySessionId {
        let session_id = self.next_session_id;
        self.next_session_id += 1;
        session_id
    }
}

#[derive(Default)]
pub struct DiscoveryMetrics {
    failed_discovery_sessions: usize,
    successful_discovery_sessions: usize,
    newly_learned_transport_infos: usize,
}

#[allow(unused, reason = "use when exposing metrics to the high-level api")]
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

        // Spawn random walkers. They start automatically and initiate discovery sessions.
        for walker_id in 0..args.discovery_config.random_walkers_count {
            DiscoveryWalker::spawn_linked(
                Some(DiscoveryActorName::new_walker(walker_id).to_string(&actor_namespace)),
                (args.clone(), store.clone(), myself.clone()),
                myself.clone().into(),
                pool.clone(),
            )
            .await?;
        }

        Ok(DiscoveryManagerState {
            actor_namespace,
            args,
            store,
            pool,
            next_session_id: 0,
            sessions: HashMap::new(),
            metrics: DiscoveryMetrics::default(),
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
            ToDiscoveryManager::StartSession(node_id, walker_ref) => {
                // Sessions we've initiated ourselves are always connected to a particular walker.
                // Each walker can only ever run max. one discovery sessions at a time.
                let session_id = state.next_session_id();
                let walker_id = DiscoveryActorName::from_actor_ref(&walker_ref).walker_id();

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
                        DiscoverySessionArguments::Connect { walker_ref },
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
                        DiscoverySessionArguments::Accept { connection },
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
            ToDiscoveryManager::FinishSession(session_id, discovery_result) => {
                state.metrics.successful_discovery_sessions += 1;
                state.sessions.remove(&session_id);

                // Ignore missing address book actor or receive errors. This means the system is
                // shutting down.
                let address_book_ref = {
                    let Some(actor) =
                        registry::where_is(with_namespace(ADDRESS_BOOK, &state.actor_namespace))
                    else {
                        return Ok(());
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
                        return Ok(()); // Ignore actor send failure
                    };

                    match result {
                        Ok(is_new_info) => {
                            if is_new_info {
                                state.metrics.newly_learned_transport_infos += 1;
                            }
                        }
                        Err(_) => {
                            // If this insertion fails we know that some of the given information
                            // was invalid (eg. wrong signature) and we stop here as we can't trust
                            // this node anymore.
                            //
                            // @TODO: Later we want to "rate" this node as misbehaving.
                            return Ok(());
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
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
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
                        // escalate the error to a parent instance.
                        return Err(ActorProcessingErr::from(format!(
                            "walker actor {walker_id} failed with error: {error}"
                        )));
                    }
                    DiscoveryActorName::Session { session_id }
                    | DiscoveryActorName::AcceptedSession { session_id } => {
                        state.metrics.failed_discovery_sessions += 1;

                        // Clean up failed actors as they likely did not send a "FinishSession"
                        // message to us.
                        state.sessions.remove(&session_id);
                    }
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[derive(Debug)]
struct DiscoveryProtocolHandler<T> {
    manager_ref: ActorRef<ToDiscoveryManager<T>>,
}

impl<T> ProtocolHandler for DiscoveryProtocolHandler<T>
where
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
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
