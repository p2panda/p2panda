// SPDX-License-Identifier: MIT OR Apache-2.0

//! Join a set of peers on a gossip topic.
use iroh::EndpointId;
use iroh_gossip::api::GossipSender as IrohGossipSender;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tracing::debug;

pub enum ToGossipJoiner {
    /// Join the given set of peers.
    JoinPeers(Vec<EndpointId>),
}

pub struct GossipJoinerState {
    sender: Option<IrohGossipSender>,
}

#[derive(Default)]
pub struct GossipJoiner;

impl ThreadLocalActor for GossipJoiner {
    type State = GossipJoinerState;
    type Msg = ToGossipJoiner;
    type Arguments = IrohGossipSender;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        sender: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(GossipJoinerState {
            sender: Some(sender),
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        drop(state.sender.take());
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToGossipJoiner::JoinPeers(peers) => {
                debug!("received join peers message with peers: {:?}", peers);
                if let Some(sender) = &mut state.sender {
                    if !peers.is_empty() {
                        sender.join_peers(peers).await?;
                        debug!("told gossip to join peers");
                    }
                }
            }
        }
        Ok(())
    }
}
