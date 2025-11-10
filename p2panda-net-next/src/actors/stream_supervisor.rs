// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stream supervisor actor.
//!
//! This actor is responsible for spawning the sync, gossip and stream actors. It also performs
//! supervision of the spawned actors, restarting them in the event of failure.
//!
//! ```plain
//! - "Stream" Supervisor
//!     - "Sync Manager" Actor
//!     - "Gossip" Actor
//!     - "Stream" Actor
//! ```
//!
//! An iroh `Endpoint` is held as part of the internal state of this actor. This allows an
//! `Endpoint` to be passed into the gossip actor in the event that it needs to be respawned (since
//! the `Endpoint` is needed to instantiate iroh `Gossip`).
use std::collections::HashMap;
use std::sync::mpsc::sync_channel;

/// Stream supervisor actor name.
pub const STREAM_SUPERVISOR: &str = "net.stream_supervisor";

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{
    Actor, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call, cast, registry,
};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;
use tracing::{debug, warn};

use crate::TopicId;
use crate::actors::gossip::{GOSSIP, Gossip, ToGossip};
use crate::actors::iroh::{IROH_ENDPOINT, ToIrohEndpoint};
use crate::actors::stream::{STREAM, Stream, ToStream};
use crate::actors::sync::{SYNC_MANAGER, SyncManager};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};
use crate::args::ApplicationArguments;
use crate::network::{FromNetwork, ToNetwork};
use crate::topic_streams::{EphemeralStream, EphemeralStreamSubscription};
use crate::utils::to_public_key;

pub struct StreamSupervisorState {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    endpoint: iroh::Endpoint,
    sync_manager_actor: ActorRef<()>,
    sync_manager_actor_failures: u16,
    gossip_actor: ActorRef<ToGossip>,
    gossip_actor_failures: u16,
    stream_actor: ActorRef<ToStream>,
    stream_actor_failures: u16,
}

#[derive(Default)]
pub struct StreamSupervisor;

impl ThreadLocalActor for StreamSupervisor {
    type State = StreamSupervisorState;
    type Msg = ();
    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let actor_namespace = generate_actor_namespace(&args.public_key);

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
        let (sync_manager_actor, _) = SyncManager::spawn_linked(
            Some(with_namespace(SYNC_MANAGER, &actor_namespace)),
            (),
            myself.clone().into(),
            args.root_thread_pool.clone(),
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
        let (stream_actor, _) = Stream::spawn_linked(
            Some(with_namespace(STREAM, &actor_namespace)),
            (
                actor_namespace.clone(),
                sync_manager_actor.clone(),
                gossip_actor.clone(),
            ),
            myself.into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        let state = StreamSupervisorState {
            actor_namespace,
            args,
            endpoint,
            sync_manager_actor,
            sync_manager_actor_failures: 0,
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
                    if name == with_namespace(SYNC_MANAGER, &actor_namespace) {
                        warn!(
                            "{STREAM_SUPERVISOR} actor: {SYNC_MANAGER} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the sync actor.
                        let (sync_manager_actor, _) = SyncManager::spawn_linked(
                            Some(with_namespace(SYNC_MANAGER, &actor_namespace)),
                            (),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
                        )
                        .await?;

                        state.sync_manager_actor_failures += 1;
                        state.sync_manager_actor = sync_manager_actor;
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
                        let (stream_actor, _) = Stream::spawn_linked(
                            Some(with_namespace(STREAM, &actor_namespace)),
                            (
                                state.actor_namespace.clone(),
                                state.sync_manager_actor.clone(),
                                state.gossip_actor.clone(),
                            ),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
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
    use ractor::actor::actor_cell::ActorStatus;
    use ractor::registry;
    use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
    use serial_test::serial;
    use tokio::time::{Duration, sleep};

    use crate::actors::gossip::GOSSIP;
    use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint};
    use crate::actors::stream::STREAM;
    use crate::actors::sync::SYNC_MANAGER;
    use crate::actors::{generate_actor_namespace, with_namespace};
    use crate::args::ArgsBuilder;
    use crate::utils::to_public_key;

    use super::{STREAM_SUPERVISOR, StreamSupervisor};

    #[tokio::test]
    #[serial]
    async fn child_actors_started() {
        let args = ArgsBuilder::new([1; 32]).build();
        let actor_namespace = generate_actor_namespace(&args.public_key);

        // Spawn the iroh endpoint actor.
        //
        // We spawn this here because the stream supervisor depends on it.
        let (endpoint_actor, endpoint_actor_handle) = IrohEndpoint::spawn(
            Some(with_namespace(IROH_ENDPOINT, &actor_namespace)),
            args.clone(),
            args.root_thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn the stream supervisor.
        let (stream_supervisor, stream_supervisor_handle) = StreamSupervisor::spawn(
            Some(with_namespace(STREAM_SUPERVISOR, &actor_namespace)),
            args.clone(),
            args.root_thread_pool.clone(),
        )
        .await
        .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        // Ensure all actors spawned by the stream supervisor are running.
        let sync_manager = registry::where_is(with_namespace(SYNC_MANAGER, &actor_namespace));
        assert!(sync_manager.is_some());
        assert_eq!(sync_manager.unwrap().get_status(), ActorStatus::Running);

        let gossip_actor = registry::where_is(with_namespace(GOSSIP, &actor_namespace));
        assert!(gossip_actor.is_some());
        assert_eq!(gossip_actor.unwrap().get_status(), ActorStatus::Running);

        let stream_actor = registry::where_is(with_namespace(STREAM, &actor_namespace));
        assert!(stream_actor.is_some());
        assert_eq!(stream_actor.unwrap().get_status(), ActorStatus::Running);

        stream_supervisor.stop(None);
        stream_supervisor_handle.await.unwrap();
        endpoint_actor.stop(None);
        endpoint_actor_handle.await.unwrap();
    }
}
