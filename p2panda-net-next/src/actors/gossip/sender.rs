// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gossip sender actor which holds the topic sender, receives local messages and broadcasts them
//! to the overlay.
//!
//! The actor first waits for a signal specifying that the gossip topic has been joined. Any
//! broadcast messages received before the join signal are queued internally (by the actor) and are
//! then processed after the signal has been received.
use iroh_gossip::api::GossipSender as IrohGossipSender;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tokio::sync::oneshot::Receiver as OneshotReceiver;

pub enum ToGossipSender {
    /// Wait for a signal specifying that the gossip topic has been joined.
    WaitUntilJoined(OneshotReceiver<u8>),

    /// Broadcast the given bytes into the gossip topic overlay.
    Broadcast(Vec<u8>),
}

pub struct GossipSenderState {
    sender: Option<IrohGossipSender>,
}

#[derive(Default)]
pub struct GossipSender;

impl ThreadLocalActor for GossipSender {
    type State = GossipSenderState;
    type Msg = ToGossipSender;
    type Arguments = (IrohGossipSender, OneshotReceiver<u8>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (sender, joined) = args;

        // Invoke the handler to wait for the gossip overlay to be joined.
        let _ = myself.cast(ToGossipSender::WaitUntilJoined(joined));

        Ok(GossipSenderState {
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
            ToGossipSender::WaitUntilJoined(joined) => {
                // This line of code blocks until the join signal is received. It's important to
                // only start broadcasting messages once the overlay has been joined, otherwise
                // those messages will simply vanish into the primordial void.
                //
                // Any messages sent to this actor in the meantime are queued and processed once
                // the join signal is received.
                let _ = joined.await;
            }
            ToGossipSender::Broadcast(bytes) => {
                if let Some(sender) = &mut state.sender {
                    sender.broadcast(bytes.into()).await?;
                }
            }
        }
        Ok(())
    }
}
