// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::marker::PhantomData;

use p2panda_discovery::DiscoveryResult;
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::random_walk::{RandomWalker, RandomWalkerConfig};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call};
use rand_chacha::ChaCha20Rng;

use crate::actors::discovery::session::DiscoverySession;
use crate::actors::discovery::walker::{DiscoveryWalker, ToDiscoveryWalker};
use crate::args::ApplicationArguments;
use crate::{NodeId, NodeInfo};

pub const DISCOVERY_MANAGER: &str = "net.discovery.manager";

pub enum ToDiscoveryManager<T> {
    /// Initiate a discovery session with the given node.
    ///
    /// A reference to the walker actor which initiated this session is kept, so the result of the
    /// session can be reported back to it.
    StartSession(NodeId, ActorRef<ToDiscoveryWalker<T>>),
}

pub struct DiscoveryManagerState<S, T> {
    args: ApplicationArguments,
    store: S,
    pool: ThreadLocalActorSpawner,
    _marker: PhantomData<(S, T)>,
}

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
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
    T: Send + 'static,
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
                    (node_id, walker_ref, state.args.clone(), state.store.clone()),
                    myself.clone().into(),
                    state.pool.clone(),
                )
                .await?;
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
        // @TODO: Manage walkers and sessions.
        Ok(())
    }
}
