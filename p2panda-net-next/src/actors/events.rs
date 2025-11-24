// SPDX-License-Identifier: MIT OR Apache-2.0

//! Events actor.
//!
//! Receives events from other actors, aggregating and enriching them before informing upstream
//! subscribers.
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, call, registry};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::info;

use crate::NodeInfo;
use crate::actors::address_book::watch_node_info;
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::args::ApplicationArguments;
use crate::events::{EventsReceiver, EventsSender, NetworkEvent};

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
}

pub struct EventsState {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    tx: EventsSender,
    watch_addr_handle: Option<JoinHandle<()>>,
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
                if let Some(transport_info) = node_info.transports {
                    info!(%transport_info, "updated our address");
                } else {
                    info!("we're currently 'not reachable'");
                }
            }
            ToEvents::Subscribe(reply) => {
                let _ = reply.send(state.tx.subscribe());
            }
        }

        Ok(())
    }
}

pub async fn subscribe_to_network_events(
    actor_namespace: &ActorNamespace,
) -> Result<broadcast::Receiver<NetworkEvent>, SubscribeError> {
    let actor_ref = registry::where_is(with_namespace(EVENTS, &actor_namespace))
        .map(ActorRef::<ToEvents>::from)
        .ok_or(SubscribeError::ActorNotAvailable)?;
    let rx =
        call!(actor_ref, ToEvents::Subscribe).map_err(|_| SubscribeError::SubscriptionFailed)?;
    Ok(rx)
}

#[derive(Debug, Error)]
pub enum SubscribeError {
    #[error("events actor is not available")]
    ActorNotAvailable,

    #[error("could not subscribe to events actor")]
    SubscriptionFailed,
}
