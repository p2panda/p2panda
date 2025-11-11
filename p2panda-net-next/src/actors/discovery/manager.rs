// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::convert::Infallible;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::time::Instant;

use iroh::protocol::ProtocolHandler;
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::random_walk::{RandomWalker, RandomWalkerConfig};
use p2panda_discovery::{DiscoveryResult, traits};
use ractor::concurrency::JoinHandle;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{
    Actor, ActorCell, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call,
};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::TopicId;
use crate::actors::discovery::DISCOVERY_PROTOCOL_ID;
use crate::actors::discovery::session::{
    DISCOVERY_SESSION, DiscoverySession, DiscoverySessionArguments, DiscoverySessionId,
};
use crate::actors::discovery::walker::{DISCOVERY_WALKER, DiscoveryWalker, ToDiscoveryWalker};
use crate::actors::iroh::register_protocol;
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::utils::to_public_key;

pub const DISCOVERY_MANAGER: &str = "net.discovery.manager";

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
    _marker: PhantomData<T>,
}

impl<S, T> DiscoveryManagerState<S, T> {
    pub fn next_session_id(&mut self) -> DiscoverySessionId {
        let session_id = self.next_session_id;
        self.next_session_id += 1;
        session_id
    }
}

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
                // @TODO: Insert result in address book.
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
            SupervisionEvent::ActorStarted(actor) => {
                let actor = DiscoveryActorName::from_actor_cell(&actor);
                // @TODO
            }
            SupervisionEvent::ActorTerminated(actor, _, _) => {
                let actor = DiscoveryActorName::from_actor_cell(&actor);
                // @TODO
            }
            SupervisionEvent::ActorFailed(actor, error) => {
                let actor = DiscoveryActorName::from_actor_cell(&actor);
                // @TODO
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
            .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;
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

/// Helper to extract information about an actor given it's name (just a string).
#[derive(Debug, PartialEq)]
enum DiscoveryActorName {
    Walker { walker_id: usize },
    Session { session_id: DiscoverySessionId },
    AcceptedSession { session_id: DiscoverySessionId },
}

impl DiscoveryActorName {
    pub fn new_walker(walker_id: usize) -> Self {
        Self::Walker { walker_id }
    }

    pub fn new_session(session_id: DiscoverySessionId) -> Self {
        Self::Session { session_id }
    }

    pub fn new_accept_session(session_id: DiscoverySessionId) -> Self {
        Self::AcceptedSession { session_id }
    }

    fn from_string(name: &str) -> Self {
        if name.contains(DISCOVERY_WALKER) {
            Self::Walker {
                walker_id: Self::extract_id(name) as usize,
            }
        } else if name.contains(DISCOVERY_SESSION) {
            Self::Session {
                session_id: Self::extract_id(name),
            }
        } else {
            unreachable!("actors have either walker or session name")
        }
    }

    pub fn from_actor_cell(actor_cell: &ActorCell) -> Self {
        Self::from_string(without_namespace(
            &actor_cell.get_name().expect("actor needs to have a name"),
        ))
    }

    pub fn from_actor_ref<T>(actor_ref: &ActorRef<T>) -> Self {
        Self::from_string(without_namespace(
            &actor_ref.get_name().expect("actor needs to have a name"),
        ))
    }

    fn extract_id(actor_name: &str) -> u64 {
        let Some((_, suffix)) = actor_name.rsplit_once('.') else {
            unreachable!("actors have all the same name pattern")
        };
        u64::from_str_radix(suffix, 10).expect("suffix is a number")
    }

    pub fn session_id(&self) -> DiscoverySessionId {
        match self {
            DiscoveryActorName::Session { session_id } => *session_id,
            DiscoveryActorName::AcceptedSession { session_id } => *session_id,
            _ => unreachable!("should only be called on session actors"),
        }
    }

    pub fn walker_id(&self) -> usize {
        match self {
            DiscoveryActorName::Walker { walker_id } => *walker_id,
            _ => unreachable!("should only be called on walker actors"),
        }
    }

    pub fn to_string(&self, actor_namespace: &ActorNamespace) -> String {
        match self {
            DiscoveryActorName::Walker { walker_id } => {
                with_namespace(&format!("{DISCOVERY_WALKER}.{walker_id}"), &actor_namespace)
            }
            DiscoveryActorName::Session { session_id }
            | DiscoveryActorName::AcceptedSession { session_id } => with_namespace(
                &format!("{DISCOVERY_SESSION}.{session_id}"),
                &actor_namespace,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;

    use crate::actors::generate_actor_namespace;

    use super::DiscoveryActorName;

    #[test]
    fn discovery_actor_name() {
        let public_key = PrivateKey::new().public_key();
        let actor_namespace = &generate_actor_namespace(&public_key);
        let value = DiscoveryActorName::new_walker(6).to_string(actor_namespace);
        assert_eq!(
            DiscoveryActorName::from_string(&value),
            DiscoveryActorName::Walker { walker_id: 6 }
        );
    }
}
