// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use iroh::protocol::ProtocolHandler;
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::random_walk::{RandomWalker, RandomWalkerConfig};
use p2panda_discovery::{DiscoveryResult, traits};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::TopicId;
use crate::actors::discovery::DISCOVERY_PROTOCOL_ID;
use crate::actors::discovery::session::{DiscoverySession, DiscoverySessionArguments};
use crate::actors::discovery::walker::{DiscoveryWalker, ToDiscoveryWalker};
use crate::actors::iroh::register_protocol;
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

    /// Received result from a successful discovery session.
    FinishSession(DiscoveryResult<T, NodeId, NodeInfo>),
}

pub struct DiscoveryManagerState<S, T> {
    args: ApplicationArguments,
    store: S,
    pool: ThreadLocalActorSpawner,
    _marker: PhantomData<(S, T)>,
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
        let pool = ThreadLocalActorSpawner::new();

        // Accept incoming "discovery protocol" connection requests.
        register_protocol(
            DISCOVERY_PROTOCOL_ID,
            DiscoveryProtocolHandler {
                manager_ref: myself.clone(),
                args: args.clone(),
                store: store.clone(),
                pool: pool.clone(),
            },
        )?;

        // Spawn random walkers. They start automatically and initiate discovery sessions.
        for _ in 0..args.discovery_config.random_walkers_count {
            DiscoveryWalker::spawn_linked(
                None,
                (args.clone(), store.clone(), myself.clone()),
                myself.clone().into(),
                pool.clone(),
            )
            .await?;
        }

        Ok(DiscoveryManagerState {
            args,
            store,
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
            ToDiscoveryManager::StartSession(node_id, walker_ref) => {
                DiscoverySession::spawn_linked(
                    None,
                    (
                        node_id,
                        state.store.clone(),
                        myself.clone(),
                        DiscoverySessionArguments::Connect { walker_ref },
                    ),
                    myself.clone().into(),
                    state.pool.clone(),
                )
                .await?;
            }
            ToDiscoveryManager::FinishSession(discovery_result) => {
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
                // @TODO
            }
            SupervisionEvent::ActorTerminated(actor, state, reason) => {
                // @TODO
            }
            SupervisionEvent::ActorFailed(actor, error) => {
                // @TODO
            }
            _ => (),
        }

        Ok(())
    }
}

#[derive(Debug)]
struct DiscoveryProtocolHandler<S, T> {
    manager_ref: ActorRef<ToDiscoveryManager<T>>,
    args: ApplicationArguments,
    store: S,
    pool: ThreadLocalActorSpawner,
}

impl<S, T> ProtocolHandler for DiscoveryProtocolHandler<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Debug + Send + Sync + 'static,
    S::Error: StdError + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    async fn accept(
        &self,
        connection: iroh::endpoint::Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        let (_, handle) = DiscoverySession::spawn_linked(
            None,
            (
                to_public_key(connection.remote_id()),
                self.store.clone(),
                self.manager_ref.clone(),
                DiscoverySessionArguments::Accept { connection },
            ),
            self.manager_ref.clone().into(),
            self.pool.clone(),
        )
        .await
        .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;

        // Wait until discovery session ended (failed or successful).
        handle
            .await
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
