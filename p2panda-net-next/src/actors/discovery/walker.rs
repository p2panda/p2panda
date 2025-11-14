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

pub enum ToDiscoveryWalker<T> {
    NextNode(Option<DiscoveryResult<T, NodeId, NodeInfo>>),
}

pub struct DiscoveryWalkerState<S, T> {
    manager_ref: ActorRef<ToDiscoveryManager<T>>,
    walker: RandomWalker<ChaCha20Rng, S, T, NodeId, NodeInfo>,
}

pub struct DiscoveryWalker<S, T> {
    _marker: PhantomData<(S, T)>,
}

impl<S, T> Default for DiscoveryWalker<S, T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S, T> ThreadLocalActor for DiscoveryWalker<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
    T: Send + 'static,
{
    type State = DiscoveryWalkerState<S, T>;

    type Msg = ToDiscoveryWalker<T>;

    type Arguments = (ApplicationArguments, S, ActorRef<ToDiscoveryManager<T>>);

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
            ToDiscoveryWalker::NextNode(previous) => {
                // Next "random walker" step finds us another node id to connect to. If this fails
                // a critical store error occurred and we stop the actor.
                let node_id = state
                    .walker
                    .next_node(previous.as_ref())
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
                        let _ = myself.send_message(ToDiscoveryWalker::NextNode(None));
                    }
                }
            }
        }
        Ok(())
    }
}
