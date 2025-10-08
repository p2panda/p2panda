// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endpoint actor.
//!
//! This actor is responsible for creating an iroh `Endpoint` and spawning the router and
//! subscription actors. It also performs supervision of the spawned actors, restarting them in the
//! event of failure.
//!
//! The router and subscription actors are children of the endpoint actor. This design decision was
//! made because they both currently rely on an iroh `Endpoint` (for the router, gossip and sync
//! connections). If something goes wrong with the iroh `Endpoint`, the endpoint actor can be
//! respawned, recreating all children with their required dependencies.
use iroh::Endpoint as IrohEndpoint;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::router::{Router, ToRouter};
use crate::actors::subscription::{Subscription, ToSubscription};

// TODO: Remove once used.
#[allow(dead_code)]
pub struct EndpointConfig {}

pub enum ToEndpoint {}

impl Message for ToEndpoint {}

pub struct EndpointState {
    endpoint: IrohEndpoint,
    subscription_actor: ActorRef<ToSubscription>,
    subscription_actor_failures: u16,
    router_actor: ActorRef<ToRouter>,
    router_actor_failures: u16,
}

pub struct Endpoint {}

impl Actor for Endpoint {
    type State = EndpointState;
    type Msg = ToEndpoint;
    type Arguments = ();

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // TODO: Build with proper configuration.
        let endpoint = IrohEndpoint::builder().bind().await?;

        // Spawn the subscription actor.
        let (subscription_actor, _) = Actor::spawn_linked(
            Some("subscription".to_string()),
            Subscription {},
            endpoint.clone(),
            myself.clone().into(),
        )
        .await?;

        // Spawn the router actor.
        let (router_actor, _) =
            Actor::spawn_linked(Some("router".to_string()), Router {}, (), myself.into()).await?;

        let state = EndpointState {
            endpoint,
            subscription_actor,
            subscription_actor_failures: 0,
            router_actor,
            router_actor_failures: 0,
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
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
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
                match actor.get_name().as_deref() {
                    Some("subscription") => {
                        warn!("endpoint actor: subscription actor failed: {}", panic_msg);

                        // Respawn the subscription actor.
                        let (subscription_actor, _) = Actor::spawn_linked(
                            Some("subscription".to_string()),
                            Subscription {},
                            state.endpoint.clone(),
                            myself.clone().into(),
                        )
                        .await?;

                        state.subscription_actor_failures += 1;
                        state.subscription_actor = subscription_actor;
                    }
                    Some("router") => {
                        warn!("endpoint actor: router actor failed: {}", panic_msg);

                        // Respawn the router actor.
                        let (router_actor, _) = Actor::spawn_linked(
                            Some("router".to_string()),
                            Router {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.router_actor_failures += 1;
                        state.router_actor = router_actor;
                    }
                    _ => (),
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
    use tokio::time::{Duration, sleep};
    use tracing_test::traced_test;

    use super::Endpoint;

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn endpoint_child_actors_are_started() {
        let (endpoint_actor, endpoint_actor_handle) =
            Actor::spawn(Some("endpoint".to_string()), Endpoint {}, ())
                .await
                .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        endpoint_actor.stop(None);
        endpoint_actor_handle.await.unwrap();

        assert!(logs_contain(
            "endpoint actor: received ready from subscription actor"
        ));
        assert!(logs_contain(
            "endpoint actor: received ready from router actor"
        ));

        assert!(!logs_contain("actor failed"));
    }
}
