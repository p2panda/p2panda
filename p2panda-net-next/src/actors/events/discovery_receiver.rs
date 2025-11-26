// SPDX-License-Identifier: MIT OR Apache-2.0

//! Discovery events receiver.
//!
//! Receives events from the discovery manager and forwards them to the network system events
//! channel.
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;
use tracing::{error, warn};

use crate::actors::discovery::DiscoveryEvent;
use crate::events::{EventsSender, NetworkEvent};

/// Discovery events receiver actor name.
pub const DISCOVERY_EVENTS_RECEIVER: &str = "net.events.discovery_receiver";

pub enum ToDiscoveryEventsReceiver {
    /// Wait for an event on the discovery events receiver.
    WaitForEvent,
}

pub struct DiscoveryEventsReceiverState {
    events_sender: Option<EventsSender>,
    receiver: Option<broadcast::Receiver<DiscoveryEvent>>,
}

#[derive(Default)]
pub struct DiscoveryEventsReceiver;

impl ThreadLocalActor for DiscoveryEventsReceiver {
    type State = DiscoveryEventsReceiverState;
    type Msg = ToDiscoveryEventsReceiver;
    type Arguments = (EventsSender, broadcast::Receiver<DiscoveryEvent>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (events_sender, receiver) = args;

        // Invoke the handler to wait for the next event on the receiver.
        let _ = myself.cast(ToDiscoveryEventsReceiver::WaitForEvent);

        Ok(DiscoveryEventsReceiverState {
            events_sender: Some(events_sender),
            receiver: Some(receiver),
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
        if let Some(receiver) = &mut state.receiver
            && let Some(events_sender) = &mut state.events_sender
        {
            match receiver.recv().await {
                Ok(event) => {
                    // Forward the event to the network system events channel.
                    if let Err(err) = events_sender.send(NetworkEvent::Discovery(event)) {
                        warn!(
                            "failed to send discovery event to network system events channel: {}",
                            err
                        )
                    }
                }
                Err(RecvError::Closed) => {
                    error!("discovery events receiver actor: channel closed");
                    myself.stop(Some("channel closed".to_string()));
                    return Ok(());
                }
                Err(RecvError::Lagged(n)) => {
                    warn!("discovery events receiver actor: receiver lagged; skipping {n} events");
                }
            }
        }

        // Invoke the handler to wait for the next event on the receiver.
        let _ = myself.cast(ToDiscoveryEventsReceiver::WaitForEvent);

        Ok(())
    }
}
