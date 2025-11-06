// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stream supervisor actor.
//!
//! This actor is responsible for spawning the sync, gossip and stream actors. It also performs
//! supervision of the spawned actors, restarting them in the event of failure.
//!
//! An iroh `Endpoint` is held as part of the internal state of this actor. This allows an
//! `Endpoint` to be passed into the gossip actor in the event that it needs to be respawned (since
//! the `Endpoint` is needed to instantiate iroh `Gossip`).
use std::collections::HashMap;
use std::sync::mpsc::sync_channel;

/// Stream supervisor actor name.
pub const STREAM_SUPERVISOR: &str = "net.stream_supervisor";

use iroh::Endpoint as IrohEndpoint;
use ractor::{
    Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call, cast, registry,
};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;
use tracing::{debug, warn};

use crate::actors::gossip::{GOSSIP, Gossip, ToGossip};
use crate::actors::iroh::{IROH_ENDPOINT, ToIrohEndpoint};
use crate::actors::stream::{STREAM, Stream, ToStream};
use crate::actors::sync::{SYNC, Sync, ToSync};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};
use crate::network::{FromNetwork, ToNetwork};
use crate::topic_streams::{EphemeralStream, EphemeralStreamSubscription};
use crate::{TopicId, to_public_key};

pub struct StreamSupervisorState {
    actor_namespace: ActorNamespace,
    endpoint: IrohEndpoint,
    sync_actor: ActorRef<ToSync>,
    sync_actor_failures: u16,
    gossip_actor: ActorRef<ToGossip>,
    gossip_actor_failures: u16,
    stream_actor: ActorRef<ToStream>,
    stream_actor_failures: u16,
}

pub struct StreamSupervisor;

impl Actor for StreamSupervisor {
    type State = StreamSupervisorState;
    type Msg = ();
    type Arguments = ActorNamespace;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        actor_namespace: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let endpoint_actor: ActorRef<ToIrohEndpoint> =
            registry::where_is(with_namespace(IROH_ENDPOINT, &actor_namespace))
                // Something went terribly wrong if the endpoint actor is not available.
                .unwrap()
                .into();

        // TODO: We have two `unwrap`s (both here and above). The panic will be caught by the
        // endpoint supervisor and the stream supervisor will be restarted. I (glyph) believe this
        // is acceptable behaviour; we should fail completely if the stream supervisor fails too
        // many times within a given time-frame - that would indicate that something is critically
        // wrong with the endpoint / endpoint actor.
        let endpoint = call!(endpoint_actor, ToIrohEndpoint::Endpoint).unwrap();

        // Spawn the sync actor.
        let (sync_actor, _) = Actor::spawn_linked(
            Some(with_namespace(SYNC, &actor_namespace)),
            Sync,
            (),
            myself.clone().into(),
        )
        .await?;

        // Spawn the gossip actor.
        let (gossip_actor, _) = Actor::spawn_linked(
            Some(with_namespace(GOSSIP, &actor_namespace)),
            Gossip,
            endpoint.clone(),
            myself.clone().into(),
        )
        .await?;

        // Spawn the stream actor.
        let (stream_actor, _) = Actor::spawn_linked(
            Some(with_namespace(STREAM, &actor_namespace)),
            Stream,
            (
                actor_namespace.clone(),
                sync_actor.clone(),
                gossip_actor.clone(),
            ),
            myself.into(),
        )
        .await?;

        let state = StreamSupervisorState {
            actor_namespace,
            endpoint,
            sync_actor,
            sync_actor_failures: 0,
            gossip_actor,
            gossip_actor_failures: 0,
            stream_actor,
            stream_actor_failures: 0,
        };

        Ok(state)
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
                        "{STREAM_SUPERVISOR} actor: received ready from {} actor",
                        without_namespace(&name)
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                let actor_namespace = generate_actor_namespace(&to_public_key(state.endpoint.id()));

                if let Some(name) = actor.get_name().as_deref() {
                    if name == with_namespace(SYNC, &actor_namespace) {
                        warn!(
                            "{STREAM_SUPERVISOR} actor: {SYNC} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the sync actor.
                        let (sync_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(SYNC, &actor_namespace)),
                            Sync,
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.sync_actor_failures += 1;
                        state.sync_actor = sync_actor;
                    } else if name == with_namespace(GOSSIP, &actor_namespace) {
                        warn!(
                            "{STREAM_SUPERVISOR} actor: {GOSSIP} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the gossip actor.
                        let (gossip_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(GOSSIP, &actor_namespace)),
                            Gossip,
                            state.endpoint.clone(),
                            myself.clone().into(),
                        )
                        .await?;

                        state.gossip_actor_failures += 1;
                        state.gossip_actor = gossip_actor;
                    } else if name == with_namespace(STREAM, &actor_namespace) {
                        warn!(
                            "{STREAM_SUPERVISOR} actor: {STREAM} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the stream actor.
                        let (stream_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(STREAM, &actor_namespace)),
                            Stream,
                            (
                                state.actor_namespace.clone(),
                                state.sync_actor.clone(),
                                state.gossip_actor.clone(),
                            ),
                            myself.clone().into(),
                        )
                        .await?;

                        state.stream_actor_failures += 1;
                        state.stream_actor = stream_actor;
                    }
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!(
                        "{STREAM_SUPERVISOR} actor: {} actor terminated",
                        without_namespace(&name)
                    );
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use iroh::Endpoint as IrohEndpoint;
    use p2panda_core::PrivateKey;
    use ractor::Actor;
    use serial_test::serial;
    use tokio::time::{Duration, sleep};
    use tracing_test::traced_test;

    use crate::actors::endpoint::{ENDPOINT, Endpoint, EndpointConfig};
    use crate::actors::gossip::GOSSIP;
    use crate::actors::stream::STREAM;
    use crate::actors::sync::SYNC;
    use crate::actors::{generate_actor_namespace, with_namespace};
    use crate::to_public_key;

    use super::{STREAM_SUPERVISOR, StreamSupervisor};

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn stream_supervisor_child_actors_are_started() {
        let private_key = PrivateKey::new();
        let actor_namespace = generate_actor_namespace(&private_key.public_key());

        // Spawn the endpoint actor.
        //
        // We spawn this here because the stream supervisor depends on it.
        let (endpoint_actor, endpoint_actor_handle) = Actor::spawn(
            Some(with_namespace(ENDPOINT, &actor_namespace)),
            Endpoint,
            (private_key.clone(), EndpointConfig::default()),
        )
        .await
        .unwrap();

        // Spawn the stream supervisor.
        let (stream_supervisor, stream_supervisor_handle) = Actor::spawn(
            Some(STREAM_SUPERVISOR.to_string()),
            StreamSupervisor,
            actor_namespace,
        )
        .await
        .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        endpoint_actor.stop(None);
        endpoint_actor_handle.await.unwrap();
        stream_supervisor.stop(None);
        stream_supervisor_handle.await.unwrap();

        assert!(logs_contain(&format!(
            "{STREAM_SUPERVISOR} actor: received ready from {SYNC} actor"
        )));
        assert!(logs_contain(&format!(
            "{STREAM_SUPERVISOR} actor: received ready from {GOSSIP} actor"
        )));
        assert!(logs_contain(&format!(
            "{STREAM_SUPERVISOR} actor: received ready from {STREAM} actor"
        )));
        assert!(!logs_contain("actor failed"));
    }
}
