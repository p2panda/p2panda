// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endpoint supervisor actor.
//!
//! This supervisor monitors the health of the endpoint actor, as well as the stream and discovery
//! actors. If the endpoint actor fails, all child actors of the endpoint supervisor are respawned
//! (including the stream and discovery actors); this is necessary because stream and discovery
//! are indirectly reliant on a functioning endpoint actor. If either the stream or discovery actors
//! fail in isolation, they are simply respawned in a one-for-one manner.
use p2panda_core::PrivateKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::discovery::{DISCOVERY, Discovery, ToDiscovery};
use crate::actors::endpoint::{ENDPOINT, Endpoint, EndpointConfig, ToEndpoint};
use crate::actors::stream_supervisor::{STREAM_SUPERVISOR, StreamSupervisor};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};

/// Endpoint supervisor actor name.
pub const ENDPOINT_SUPERVISOR: &str = "net.endpoint_supervisor";

pub struct EndpointSupervisorState {
    actor_namespace: ActorNamespace,
    private_key: PrivateKey,
    // TODO: We need to store the config on the state so the endpoint actor can be restarted. This
    // will only be possible once we have our own custom router in place.
    //endpoint_config: EndpointConfig,
    endpoint_actor: ActorRef<ToEndpoint>,
    discovery_actor: ActorRef<ToDiscovery>,
    discovery_actor_failures: u16,
}

pub struct EndpointSupervisor;

impl Actor for EndpointSupervisor {
    type State = EndpointSupervisorState;
    type Msg = ();
    type Arguments = (PrivateKey, EndpointConfig);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (private_key, endpoint_config) = args;

        let actor_namespace = generate_actor_namespace(&private_key.public_key());

        // Spawn the endpoint actor.
        let (endpoint_actor, _) = Actor::spawn_linked(
            Some(with_namespace(ENDPOINT, &actor_namespace)),
            Endpoint,
            (private_key.clone(), endpoint_config),
            myself.clone().into(),
        )
        .await?;

        // Spawn the discovery actor.
        let (discovery_actor, _) = Actor::spawn_linked(
            Some(with_namespace(DISCOVERY, &actor_namespace)),
            Discovery {},
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the stream supervisor.
        let (stream_supervisor, _) = Actor::spawn_linked(
            Some(with_namespace(STREAM_SUPERVISOR, &actor_namespace)),
            StreamSupervisor,
            actor_namespace.clone(),
            myself.clone().into(),
        )
        .await?;

        let state = EndpointSupervisorState {
            actor_namespace,
            private_key,
            endpoint_actor,
            discovery_actor,
            discovery_actor_failures: 0,
        };

        Ok(state)
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let reason = Some("endpoint supervisor is shutting down".to_string());

        // Stop all the actors which are directly supervised by this actor.
        state.endpoint_actor.stop(reason.clone());
        state.discovery_actor.stop(reason);

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
                    debug!(
                        "{ENDPOINT_SUPERVISOR} actor: received ready from {} actor",
                        without_namespace(&name)
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if let Some(name) = actor.get_name().as_deref() {
                    if name == with_namespace(ENDPOINT, &state.actor_namespace) {
                        warn!(
                            "{ENDPOINT_SUPERVISOR} actor: {ENDPOINT} actor failed: {}",
                            panic_msg
                        );

                        // If the endpoint actor fails then we need to:
                        //
                        // 1. Stop the stream supervisor and discovery actors
                        // 2. Respawn the endpoint actor
                        // 3. Respawn the stream supervisor and discovery actors
                        state
                            .discovery_actor
                            .stop(Some("{ENDPOINT} actor failed".to_string()));

                        // Respawn the endpoint actor.
                        //let (endpoint_actor, _) = Actor::spawn_linked(
                        //    Some(with_namespace(ENDPOINT, &state.actor_namespace)),
                        //    Endpoint,
                        //    (state.private_key, state.config),
                        //    myself.clone().into(),
                        //)
                        //.await?;

                        // Respawn the discovery actor.
                        let (discovery_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(DISCOVERY, &state.actor_namespace)),
                            Discovery {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.discovery_actor_failures += 1;
                        state.discovery_actor = discovery_actor;
                    } else if name == with_namespace(DISCOVERY, &state.actor_namespace) {
                        warn!(
                            "{ENDPOINT_SUPERVISOR} actor: {DISCOVERY} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the discovery actor.
                        let (discovery_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(DISCOVERY, &state.actor_namespace)),
                            Discovery {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.discovery_actor_failures += 1;
                        state.discovery_actor = discovery_actor;
                    }
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!(
                        "{ENDPOINT_SUPERVISOR} actor: {} actor terminated",
                        without_namespace(&name)
                    );
                }
            }
            _ => (),
        }

        Ok(())
    }
}
