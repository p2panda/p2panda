// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gossip sender actor which holds the topic sender, receives local messages and broadcasts them
//! to the overlay.

use iroh_gossip::net::GossipSender as IrohGossipSender;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message};

pub enum ToGossipSender {
    Broadcast(Vec<u8>),
}

impl Message for ToGossipSender {}

pub struct GossipSenderState {
    sender: Option<IrohGossipSender>,
}

pub struct GossipSender;

impl Actor for GossipSender {
    type State = GossipSenderState;
    type Msg = ToGossipSender;
    type Arguments = IrohGossipSender;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        sender: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = GossipSenderState {
            sender: Some(sender),
        };

        Ok(state)
    }

    async fn post_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
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
        if let Some(sender) = &mut state.sender {
            let ToGossipSender::Broadcast(bytes) = message;
            sender.broadcast(bytes.into()).await?;
        }

        Ok(())
    }
}
