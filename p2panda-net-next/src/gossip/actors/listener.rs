// SPDX-License-Identifier: MIT OR Apache-2.0

//! Listen for messages from the user and forward them to the gossip sender.
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tokio::sync::mpsc::Receiver;
use tracing::trace;

use crate::gossip::actors::sender::ToGossipSender;

pub enum ToGossipListener {
    /// Wait for a message on the gossip topic channel.
    WaitForMessage,
}

pub struct GossipListenerState {
    receiver: Option<Receiver<Vec<u8>>>,
    sender_ref: ActorRef<ToGossipSender>,
}

#[derive(Default)]
pub struct GossipListener;

impl ThreadLocalActor for GossipListener {
    type State = GossipListenerState;
    type Msg = ToGossipListener;
    type Arguments = (Receiver<Vec<u8>>, ActorRef<ToGossipSender>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (receiver, sender_ref) = args;

        // Invoke the handler to wait for the first message on the receiver.
        let _ = myself.cast(ToGossipListener::WaitForMessage);

        Ok(GossipListenerState {
            receiver: Some(receiver),
            sender_ref,
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
        _message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(receiver) = &mut state.receiver {
            match receiver.recv().await {
                Some(bytes) => {
                    // Forward the message bytes to the gossip sender for broadcast.
                    let _ = state.sender_ref.cast(ToGossipSender::Broadcast(bytes));

                    // Invoke the handler to wait for the next message on the receiver.
                    let _ = myself.cast(ToGossipListener::WaitForMessage);
                }
                None => {
                    trace!("gossip listener actor: user dropped sender - channel closed");
                    myself.stop(Some("receiver channel closed".to_string()));
                }
            }
        }
        Ok(())
    }
}
