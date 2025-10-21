// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gossip recevier actor which holds the gossip topic receiver, receives overlay messages and sends
//! them to the gossip session actor.
use futures_lite::StreamExt;
use iroh_gossip::api::GossipReceiver as IrohGossipReceiver;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message};
use tracing::error;

use crate::actors::gossip::session::ToGossipSession;

pub enum ToGossipReceiver {
    /// Wait for an event on the gossip topic receiver.
    WaitForEvent,

    /// Wait for the first `NeighborUp` event on the receiver, signifying that the gossip overlay
    /// has been joined.
    WaitForJoin,
}

impl Message for ToGossipReceiver {}

pub struct GossipReceiverState {
    receiver: Option<IrohGossipReceiver>,
}

pub struct GossipReceiver {
    session: ActorRef<ToGossipSession>,
}

impl GossipReceiver {
    pub fn new(session: ActorRef<ToGossipSession>) -> Self {
        Self { session }
    }
}

impl Actor for GossipReceiver {
    type State = GossipReceiverState;
    type Msg = ToGossipReceiver;
    type Arguments = IrohGossipReceiver;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        receiver: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Invoke the handler to wait for the next event on the receiver.
        let _ = myself.cast(ToGossipReceiver::WaitForJoin);

        let state = GossipReceiverState {
            receiver: Some(receiver),
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
        drop(state.receiver.take());

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToGossipReceiver::WaitForJoin => {
                if let Some(receiver) = &mut state.receiver {
                    // Wait for the first peer connection.
                    //
                    // This will block the actor's message processing queue until the first
                    // `NeighborUp` event is received. The event is consumed by the call to
                    // `joined()`.
                    receiver.joined().await?;

                    // Inform the session actor about our direct neighbors.
                    let peers = receiver.neighbors().collect();
                    let _ = self.session.cast(ToGossipSession::ProcessJoined(peers));
                }

                // Invoke the handler to wait for the next event on the receiver.
                let _ = myself.cast(ToGossipReceiver::WaitForEvent);
            }
            ToGossipReceiver::WaitForEvent => {
                if let Some(receiver) = &mut state.receiver
                    && let Some(received) = receiver.next().await
                {
                    match received {
                        Ok(event) => {
                            // Send the event up the chain for processing.
                            let _ = self.session.cast(ToGossipSession::ProcessEvent(event));
                        }
                        Err(err) => {
                            error!("gossip receiver actor: {}", err);
                            myself.stop(Some("channel closed".to_string()));

                            return Ok(());
                        }
                    }
                }

                // Invoke the handler to wait for the next event on the receiver.
                let _ = myself.cast(ToGossipReceiver::WaitForEvent);
            }
        }

        Ok(())
    }
}
