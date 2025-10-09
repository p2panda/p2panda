// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endpoint actor.
//!
//! This actor is responsible for creating an iroh `Endpoint` with an associated `Router`,
//! registering network protocols with the `Router` and spawning the subscription actor. It also
//! performs supervision of the spawned actor, restarting it in the event of failure.
//!
//! The subscription actor is a child of the endpoint actor. This design decision was made because
//! it currently relies on an iroh `Endpoint` (for gossip and sync connections). If something goes
//! wrong with the gossip or sync actors, they can be respawned by the endpoint actor. If the
//! endpoint actor itself fails, the entire network system is shutdown.
use iroh::protocol::Router as IrohRouter;
use iroh::Endpoint as IrohEndpoint;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::subscription::{Subscription, ToSubscription};
use crate::protocols::ProtocolMap;

pub(crate) struct EndpointConfig {
    protocols: ProtocolMap,
}

impl EndpointConfig {
    pub(crate) fn new(protocols: ProtocolMap) -> Self {
        Self { protocols }
    }
}

pub(crate) enum ToEndpoint {}

impl Message for ToEndpoint {}

pub(crate) struct EndpointState {
    endpoint: IrohEndpoint,
    router: IrohRouter,
    subscription_actor: ActorRef<ToSubscription>,
    subscription_actor_failures: u16,
}

pub(crate) struct Endpoint;

impl Actor for Endpoint {
    type State = EndpointState;
    type Msg = ToEndpoint;
    type Arguments = EndpointConfig;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        config: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // TODO: Build with proper configuration.
        let endpoint = IrohEndpoint::builder().bind().await?;

        let mut protocols = config.protocols;

        let mut router_builder = IrohRouter::builder(endpoint.clone());

        // Register protocols with router.
        while let Some((identifier, handler)) = protocols.pop_first() {
            router_builder = router_builder.accept(identifier, handler);
        }

        let router = router_builder.spawn();

        // Spawn the subscription actor.
        let (subscription_actor, _) = Actor::spawn_linked(
            Some("subscription".to_string()),
            Subscription,
            endpoint.clone(),
            myself.clone().into(),
        )
        .await?;

        let state = EndpointState {
            endpoint,
            router,
            subscription_actor,
            subscription_actor_failures: 0,
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
        // Shutdown all protocol handlers and close the iroh `Endpoint`.
        state.router.shutdown().await?;

        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(actor) => {
                if let Some(name) = actor.get_name() {
                    debug!("endpoint actor: received ready from {} actor", name);
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if let Some("subscription") = actor.get_name().as_deref() {
                    warn!("endpoint actor: subscription actor failed: {}", panic_msg);

                    // Respawn the subscription actor.
                    let (subscription_actor, _) = Actor::spawn_linked(
                        Some("subscription".to_string()),
                        Subscription,
                        state.endpoint.clone(),
                        myself.clone().into(),
                    )
                    .await?;

                    state.subscription_actor_failures += 1;
                    state.subscription_actor = subscription_actor;
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!("endpoint actor: {} actor terminated", name);
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ractor::Actor;
    use serial_test::serial;
    use tokio::time::{sleep, Duration};
    use tracing_test::traced_test;

    use super::{Endpoint, EndpointConfig};

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn endpoint_child_actors_are_started() {
        let protocols = Default::default();
        let endpoint_config = EndpointConfig::new(protocols);
        let (endpoint_actor, endpoint_actor_handle) =
            Actor::spawn(Some("endpoint".to_string()), Endpoint, endpoint_config)
                .await
                .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        endpoint_actor.stop(None);
        endpoint_actor_handle.await.unwrap();

        assert!(logs_contain(
            "endpoint actor: received ready from subscription actor"
        ));

        assert!(!logs_contain("actor failed"));
    }
}
