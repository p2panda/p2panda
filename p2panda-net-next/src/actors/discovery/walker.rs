// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::marker::PhantomData;
use std::time::Duration;

use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::random_walk::{RandomWalker, RandomWalkerConfig};
use p2panda_discovery::{DiscoveryResult, DiscoveryStrategy};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, cast};
use rand_chacha::ChaCha20Rng;
use tokio::time;
use tracing::trace;

use crate::actors::discovery::{DISCOVERY_MANAGER, ToDiscoveryManager};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;

/// Actor name prefix for a walker.
pub const DISCOVERY_WALKER: &str = "net.discovery.walker";

/// Delay next step when no result was previously given.
const NO_RESULTS_DELAY: Duration = Duration::from_secs(2);

pub type SuccessRate = f32;

pub enum WalkFromHere {
    Bootstrap,
    LastSession {
        discovery_result: DiscoveryResult<NodeId, NodeInfo>,
        newly_learned_transport_infos: usize,
    },
    FailedSession {
        last_successful: Option<DiscoveryResult<NodeId, NodeInfo>>,
    },
}

impl WalkFromHere {
    pub fn success_rate(&self) -> SuccessRate {
        match self {
            WalkFromHere::Bootstrap => 1.0,
            WalkFromHere::LastSession {
                discovery_result,
                newly_learned_transport_infos,
            } => {
                *newly_learned_transport_infos as f32
                    / discovery_result.node_transport_infos.len() as f32
            }
            WalkFromHere::FailedSession { .. } => 0.0,
        }
    }

    pub fn next_node_args(&self) -> Option<&DiscoveryResult<NodeId, NodeInfo>> {
        match self {
            WalkFromHere::Bootstrap => None,
            WalkFromHere::LastSession {
                discovery_result, ..
            } => Some(discovery_result),
            WalkFromHere::FailedSession { last_successful } => last_successful.as_ref(),
        }
    }
}

pub enum ToDiscoveryWalker {
    NextNode(WalkFromHere),
}

pub struct DiscoveryWalkerState<S> {
    manager_ref: ActorRef<ToDiscoveryManager>,
    walker: RandomWalker<ChaCha20Rng, S, NodeId, NodeInfo>,
}

pub struct DiscoveryWalker<S> {
    _marker: PhantomData<S>,
}

impl<S> Default for DiscoveryWalker<S> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S> ThreadLocalActor for DiscoveryWalker<S>
where
    S: AddressBookStore<NodeId, NodeInfo> + Clone + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
{
    type State = DiscoveryWalkerState<S>;

    type Msg = ToDiscoveryWalker;

    type Arguments = (ApplicationArguments, S, ActorRef<ToDiscoveryManager>);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, store, manager_ref) = args;
        Ok(DiscoveryWalkerState {
            manager_ref,
            walker: RandomWalker::from_config(
                args.public_key,
                store,
                args.rng,
                RandomWalkerConfig {
                    reset_walk_probability: args.discovery_config.reset_walk_probability,
                },
            ),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToDiscoveryWalker::NextNode(walk_from_here) => {
                // Next "random walker" step finds us another node id to connect to. If this fails
                // a critical store error occurred and we stop the actor.
                let node_id = state
                    .walker
                    .next_node(walk_from_here.next_node_args())
                    .await
                    .map_err(|err| ActorProcessingErr::from(err.to_string()))?;

                match node_id {
                    // Tell manager to launch a discovery session with this node. When session
                    // finished it will "call back" with a result and we can continue our walk.
                    Some(node_id) => {
                        if cast!(
                            state.manager_ref,
                            ToDiscoveryManager::InitiateSession(node_id, myself)
                        )
                        .is_err()
                        {
                            trace!(
                                "parent {DISCOVERY_MANAGER} actor not available, probably winding down"
                            );
                        }
                    }
                    // When walker replied with no value we can assume that the address book is
                    // empty. In this case delay the next iteration to lower the activity,
                    // hopefully some other process will add entries in the address book soon.
                    None => {
                        time::sleep(NO_RESULTS_DELAY).await;
                        let _ = myself
                            .send_message(ToDiscoveryWalker::NextNode(WalkFromHere::Bootstrap));
                    }
                }
            }
        }
        Ok(())
    }
}
