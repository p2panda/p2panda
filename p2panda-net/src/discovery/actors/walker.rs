// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use p2panda_discovery::random_walk::{RandomWalker, RandomWalkerConfig};
use p2panda_discovery::{DiscoveryResult, DiscoveryStrategy};
use p2panda_store_next::SqliteStore;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, cast};
use rand_chacha::ChaCha20Rng;
use tokio::sync::Notify;
use tokio::time;
use tracing::trace;

use crate::NodeId;
use crate::addrs::NodeInfo;
use crate::discovery::DiscoveryConfig;
use crate::discovery::actors::ToDiscoveryManager;
use crate::discovery::backoff::{Backoff, Config as BackoffConfig};

/// Delay next step when no result was previously given.
const NO_RESULTS_DELAY: Duration = Duration::from_secs(2);

/// Increment the backoff if success rate falls under this threshold.
///
/// If we're reaching a higher value again, the backoff will be reset.
const SUCCESS_RATE_THRESHOLD: SuccessRate = 0.02; // 2% new results

/// Success metric for last discovery session.
///
/// If all discovered transport infos in that last session were "new" to us, the success rate is
/// 1.0. If last session failed it's 0.0.
pub type SuccessRate = f32;

pub enum WalkFromHere {
    /// Initiate random walk, starting from randomly picked bootstrap node.
    ///
    /// If no bootstrap nodes are available, pick any other random node.
    Bootstrap,

    /// Continue random walk, feeding the walker with information about the last successful
    /// discovery session which might inform it's behaviour for the next step.
    LastSession {
        discovery_result: DiscoveryResult<NodeId, NodeInfo>,
        newly_learned_transport_infos: usize,
    },

    /// Continue random walk after a failed session.
    ///
    /// We don't have any new information to give to the walker, if available, we give it the
    /// results from the last successful discovery session.
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
                    / discovery_result.transport_infos.len() as f32
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

pub struct DiscoveryWalkerState {
    manager_ref: ActorRef<ToDiscoveryManager>,
    walker: RandomWalker<ChaCha20Rng, SqliteStore<'static>, NodeId, NodeInfo>,
    backoff: Backoff,
    walker_reset: Arc<Notify>,
}

#[derive(Default)]
pub struct DiscoveryWalker;

impl ThreadLocalActor for DiscoveryWalker {
    type State = DiscoveryWalkerState;

    type Msg = ToDiscoveryWalker;

    type Arguments = (
        NodeId,
        DiscoveryConfig,
        SqliteStore<'static>,
        ChaCha20Rng,
        Arc<Notify>,
        ActorRef<ToDiscoveryManager>,
    );

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (my_node_id, config, store, rng, walker_reset, manager_ref) = args;
        Ok(DiscoveryWalkerState {
            manager_ref,
            walker: RandomWalker::from_config(
                my_node_id,
                store,
                rng.clone(),
                RandomWalkerConfig {
                    reset_walk_probability: config.reset_walk_probability,
                },
            ),
            backoff: Backoff::new(BackoffConfig::default(), rng),
            walker_reset,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToDiscoveryWalker::NextNode(mut walk_from_here) => {
                // We use a simple incremental backoff logic to determine if this walker can slow
                // down when it doesn't bring any new results anymore.
                if walk_from_here.success_rate() < SUCCESS_RATE_THRESHOLD {
                    state.backoff.increment();
                } else {
                    // If there's a new wave of information we make the walker faster again. This
                    // should help us to adapt to changing network dynamics.
                    state.backoff.reset();
                }

                // Wait until backoff has finished or we've received a "reset" signal from the
                // outside.
                tokio::select! {
                    _ = state.walker_reset.notified() => {
                        trace!("received notification to reset walker and backoff");
                        walk_from_here = WalkFromHere::Bootstrap;
                        state.backoff.reset();
                    }
                    _ = state.backoff.sleep() => (),
                }

                // Next "random walker" step finds us another node id to connect to. If this fails
                // a critical store error occurred and we stop the actor.
                let node_id = state
                    .walker
                    .next_node(walk_from_here.next_node_args())
                    .await?;

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
                            trace!("parent actor not available, probably winding down");
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
