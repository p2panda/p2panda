// SPDX-License-Identifier: MIT OR Apache-2.0

//! Events actor.
//!
//! Receives events from other actors, aggregating and enriching them before informing upstream
//! subscribers.
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, call, cast, registry};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::info;

use crate::NodeInfo;
use crate::actors::address_book::watch_node_info;
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::args::ApplicationArguments;
use crate::events::{EventsReceiver, EventsSender, NetworkEvent, RelayStatus};

/// Events actor name.
pub const EVENTS: &str = "net.events";

#[allow(clippy::large_enum_variant)]
pub enum ToEvents {
    /// Set up events actor.
    Initialise,

    /// Our own node info got updated.
    UpdatedNodeInfo(NodeInfo),

    /// Subscribe to system events.
    Subscribe(RpcReplyPort<EventsReceiver>),

    /// Inform all subscribers about this system event.
    Notify(NetworkEvent),
}

pub struct EventsState {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    tx: EventsSender,
    watch_addr_handle: Option<JoinHandle<()>>,
    home_relay_url: Option<iroh::RelayUrl>,
}

#[derive(Default)]
pub struct Events;

impl ThreadLocalActor for Events {
    type State = EventsState;

    type Msg = ToEvents;

    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let actor_namespace = generate_actor_namespace(&args.public_key);

        let (tx, _) = broadcast::channel(256);

        // Initialise events actor automatically.
        myself.send_message(ToEvents::Initialise)?;

        Ok(EventsState {
            actor_namespace,
            args,
            tx,
            watch_addr_handle: None,
            home_relay_url: None,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        if let Some(handle) = &state.watch_addr_handle {
            handle.abort();
        }

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToEvents::Initialise => {
                // Subscribe to our own node info to find out if transport addresses have changed.
                let mut rx =
                    watch_node_info(state.actor_namespace.clone(), state.args.public_key, false)
                        .await?;

                let watch_addr_handle = tokio::task::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        if let Some(node_info) = event.value
                            && myself
                                .send_message(ToEvents::UpdatedNodeInfo(node_info))
                                .is_err()
                        {
                            break;
                        }
                    }
                });

                state.watch_addr_handle = Some(watch_addr_handle);
            }
            ToEvents::UpdatedNodeInfo(node_info) => {
                // Find out if our relay status has changed.
                if let Ok(endpoint_addr) = iroh::EndpointAddr::try_from(node_info.clone()) {
                    let home_relay_url = endpoint_addr.relay_urls().next().cloned();
                    match (&state.home_relay_url, home_relay_url) {
                        (None, None) => {
                            // Nothing has changed.
                        }
                        (None, Some(next)) => {
                            myself.send_message(ToEvents::Notify(NetworkEvent::Relay(
                                RelayStatus::Connected(next),
                            )))?;
                        }
                        (Some(_previous), None) => {
                            myself.send_message(ToEvents::Notify(NetworkEvent::Relay(
                                RelayStatus::Disconnected,
                            )))?;
                        }
                        (Some(_previous), Some(next)) => {
                            myself.send_message(ToEvents::Notify(NetworkEvent::Relay(
                                RelayStatus::Changed(next),
                            )))?;
                        }
                    }
                };

                // Notify users about the latest transport info.
                myself.send_message(ToEvents::Notify(NetworkEvent::Transport(node_info.into())))?;
            }
            ToEvents::Subscribe(reply) => {
                let _ = reply.send(state.tx.subscribe());
            }
            ToEvents::Notify(event) => {
                info!("{:?}", event);
                let _ = state.tx.send(event);
            }
        }

        Ok(())
    }
}

pub async fn subscribe_to_network_events(
    actor_namespace: &ActorNamespace,
) -> Result<broadcast::Receiver<NetworkEvent>, SubscribeError> {
    let actor_ref = registry::where_is(with_namespace(EVENTS, actor_namespace))
        .map(ActorRef::<ToEvents>::from)
        .ok_or(SubscribeError::ActorNotAvailable)?;
    let rx = call!(actor_ref, ToEvents::Subscribe).map_err(|_| SubscribeError::ActorFailed)?;
    Ok(rx)
}

pub(crate) async fn notify_subscribers(
    actor_namespace: &ActorNamespace,
    event: NetworkEvent,
) -> Result<(), SubscribeError> {
    let actor_ref = registry::where_is(with_namespace(EVENTS, actor_namespace))
        .map(ActorRef::<ToEvents>::from)
        .ok_or(SubscribeError::ActorNotAvailable)?;
    cast!(actor_ref, ToEvents::Notify(event)).map_err(|_| SubscribeError::ActorFailed)?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum SubscribeError {
    #[error("events actor is not available")]
    ActorNotAvailable,

    #[error("events actor failed handling message")]
    ActorFailed,
}
