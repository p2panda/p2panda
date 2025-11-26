// SPDX-License-Identifier: MIT OR Apache-2.0

//! Events actor.
//!
//! Receives events from other actors, aggregating and enriching them before informing upstream
//! subscribers.
mod discovery_receiver;

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call, registry};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::NodeInfo;
use crate::actors::address_book::watch_node_info;
use crate::actors::discovery::{DISCOVERY_MANAGER, DiscoveryEvent, ToDiscoveryManager};
use crate::actors::events::discovery_receiver::{
    DISCOVERY_EVENTS_RECEIVER, DiscoveryEventsReceiver,
};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};
use crate::args::ApplicationArguments;
use crate::events::{EventsReceiver, EventsSender, NetworkEvent, RelayStatus};

/// Events actor name.
pub const EVENTS: &str = "net.events";

#[allow(clippy::large_enum_variant)]
pub enum ToEvents {
    /// Set up events actor.
    Initialise,

    /// Subscribe to system events.
    Subscribe(RpcReplyPort<EventsReceiver>),

    /// Subscribe to events from the discovery system.
    SubscribeDiscovery,

    /// Inform all subscribers about this system event.
    Notify(NetworkEvent),

    /// Our own node info got updated.
    UpdatedNodeInfo(NodeInfo),
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
                // Subscribe to event subsystems.
                //
                // These events will be sent into the main network system events channel.
                myself.send_message(ToEvents::SubscribeDiscovery)?;

                let _ = reply.send(state.tx.subscribe());
            }
            ToEvents::SubscribeDiscovery => {
                let rx = subscribe_to_discovery_events(&state.actor_namespace).await?;

                // Spawn the discovery events receiver.
                //
                // This actor is responsible for receiving discovery events and sending them to the
                // network system event channel; this allows us to unify all system events into a
                // single channel for the subscriber.
                let (_discovery_events_receiver_actor, _) = DiscoveryEventsReceiver::spawn_linked(
                    Some(with_namespace(
                        DISCOVERY_EVENTS_RECEIVER,
                        &state.actor_namespace,
                    )),
                    (state.tx.clone(), rx),
                    myself.clone().into(),
                    state.args.root_thread_pool.clone(),
                )
                .await?;
            }
            ToEvents::Notify(event) => {
                info!("{:?}", event);
                let _ = state.tx.send(event);
            }
        }

        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(actor) => {
                if let Some(name) = actor.get_name() {
                    debug!(
                        "{EVENTS} actor: received ready from {} actor",
                        without_namespace(&name)
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if let Some(name) = actor.get_name().as_deref()
                    && name == with_namespace(DISCOVERY_EVENTS_RECEIVER, &state.actor_namespace)
                {
                    warn!(
                        "{EVENTS} actor: {DISCOVERY_EVENTS_RECEIVER} actor failed: {}",
                        panic_msg
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!(
                        "{EVENTS} actor: {} actor terminated",
                        without_namespace(&name)
                    );
                }
            }
            _ => (),
        }

        Ok(())
    }
}

async fn subscribe_to_discovery_events(
    actor_namespace: &ActorNamespace,
) -> Result<broadcast::Receiver<DiscoveryEvent>, SubscribeError> {
    let actor_ref = registry::where_is(with_namespace(DISCOVERY_MANAGER, actor_namespace))
        .map(ActorRef::<ToDiscoveryManager>::from)
        .ok_or(SubscribeError::ActorNotAvailable)?;
    let rx =
        call!(actor_ref, ToDiscoveryManager::Events).map_err(|_| SubscribeError::ActorFailed)?;
    Ok(rx)
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

#[derive(Debug, Error)]
pub enum SubscribeError {
    #[error("events actor is not available")]
    ActorNotAvailable,

    #[error("events actor failed handling message")]
    ActorFailed,
}
