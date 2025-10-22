// SPDX-License-Identifier: MIT OR Apache-2.0

//! Join a set of peers on a gossip topic.
use iroh::NodeId;
use iroh_gossip::api::GossipSender as IrohGossipSender;
use ractor::{Actor, ActorProcessingErr, ActorRef};

pub enum ToGossipJoiner {
    /// Join the given set of peers.
    JoinPeers(Vec<NodeId>),
}

pub struct GossipJoinerState {
    sender: Option<IrohGossipSender>,
}

pub struct GossipJoiner;

impl Actor for GossipJoiner {
    type State = GossipJoinerState;
    type Msg = ToGossipJoiner;
    type Arguments = IrohGossipSender;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        sender: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = GossipJoinerState {
            sender: Some(sender),
        };
        Ok(state)
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
                if let Some(sender) = &mut state.sender {
                    sender.join_peers(peers).await?;
                }
                Ok(())
            }
        }
    }
}
