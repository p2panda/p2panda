// SPDX-License-Identifier: MIT OR Apache-2.0

//! Listen for messages from the user and forward them to the gossip sender.
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio::sync::mpsc::Receiver;
use tracing::warn;

use crate::actors::gossip::sender::ToGossipSender;

pub enum ToGossipListener {
    /// Wait for a message on the gossip topic channel.
    WaitForMessage,
}

pub struct GossipListenerState {
    receiver: Option<Receiver<Vec<u8>>>,
}

pub struct GossipListener {
    sender: ActorRef<ToGossipSender>,
}

impl GossipListener {
    pub fn new(sender: ActorRef<ToGossipSender>) -> Self {
        Self { sender }
    }
}

impl Actor for GossipListener {
    type State = GossipListenerState;
    type Msg = ToGossipListener;
    type Arguments = Receiver<Vec<u8>>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        receiver: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Invoke the handler to wait for the first message on the receiver.
        let _ = myself.cast(ToGossipListener::WaitForMessage);

        let state = GossipListenerState {
            receiver: Some(receiver),
        };

        Ok(state)
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
                    let _ = self.sender.cast(ToGossipSender::Broadcast(bytes));
                }
                None => {
                    warn!("gossip listener actor: user dropped sender - channel closed");
                    myself.stop(Some("receiver channel closed".to_string()));

                    return Ok(());
                }
            }
        }

        // Invoke the handler to wait for the next message on the receiver.
        let _ = myself.cast(ToGossipListener::WaitForMessage);

        Ok(())
    }
}
