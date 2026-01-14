// SPDX-License-Identifier: MIT OR Apache-2.0

//! Join a set of nodes on a gossip topic.
use iroh_gossip::api::GossipSender as IrohGossipSender;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

pub enum ToGossipJoiner {
    /// Join the given set of nodes.
    JoinNodes(Vec<iroh::EndpointId>),
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
            ToGossipJoiner::JoinNodes(nodes) => {
                if let Some(sender) = &mut state.sender {
                    sender.join_peers(nodes).await?;
                }
            }
        }
        Ok(())
    }
}
