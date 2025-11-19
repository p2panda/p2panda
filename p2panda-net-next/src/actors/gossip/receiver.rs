// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gossip recevier actor which holds the gossip topic receiver, receives overlay messages and sends
//! them to the gossip session actor.
use futures_util::StreamExt;
use iroh_gossip::api::GossipReceiver as IrohGossipReceiver;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tokio::time::Instant;
use tracing::{debug, error};

use crate::actors::gossip::session::ToGossipSession;

#[derive(Debug)]
pub enum ToGossipReceiver {
    /// Wait for an event on the gossip topic receiver.
    WaitForEvent,

    /// Wait for the first `NeighborUp` event on the receiver, signifying that the gossip overlay
    /// has been joined.
    WaitForJoin,
}

pub struct GossipReceiverState {
    receiver: Option<IrohGossipReceiver>,
    session_ref: ActorRef<ToGossipSession>,
}

#[derive(Default)]
pub struct GossipReceiver;

impl ThreadLocalActor for GossipReceiver {
    type State = GossipReceiverState;
    type Msg = ToGossipReceiver;
    type Arguments = (IrohGossipReceiver, ActorRef<ToGossipSession>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (receiver, session_ref) = args;

        // Invoke the handler to wait for the next event on the receiver.
        let _ = myself.cast(ToGossipReceiver::WaitForJoin);

        Ok(GossipReceiverState {
            receiver: Some(receiver),
            session_ref,
        })
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
        debug!("{:?}", message);

        match message {
            ToGossipReceiver::WaitForJoin => {
                if let Some(receiver) = &mut state.receiver {
                    debug!("waiting to join gossip");

                    // Wait for the first peer connection.
                    //
                    // This will block the actor's message processing queue until the first
                    // `NeighborUp` event is received. The event is consumed by the call to
                    // `joined()`.
                    receiver.joined().await?;

                    debug!("receiver.joiner returned");

                    // Inform the session actor about our direct neighbors.
                    let peers = receiver.neighbors().collect();
                    let _ = state
                        .session_ref
                        .cast(ToGossipSession::ProcessJoined(peers));
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
                            let _ = state.session_ref.cast(ToGossipSession::ProcessEvent(event));
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
