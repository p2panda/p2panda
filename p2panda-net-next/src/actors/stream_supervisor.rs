// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stream supervisor actor.
//!
//! This actor is responsible for spawning the gossip and streams actors. It also performs
//! supervision of the spawned actors, restarting them in the event of failure.
//!
//! ```plain
//! - "Stream" Supervisor
//!     - "Gossip" Actor
//!     - "Eventually Consistent Streams" Actor
//!     - "Ephemeral Streams" Actor
//! ```
//!
//! An iroh `Endpoint` is held as part of the internal state of this actor. This allows an
//! `Endpoint` to be passed into the gossip actor in the event that it needs to be respawned (since
//! the `Endpoint` is needed to instantiate iroh `Gossip`).

/// Stream supervisor actor name.
pub const STREAM_SUPERVISOR: &str = "net.stream_supervisor";

use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_sync::traits::SyncManager;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, SupervisionEvent, call, registry};
use tracing::{trace, warn};

use crate::TopicId;
use crate::actors::gossip::{GOSSIP, Gossip, ToGossip};
use crate::actors::iroh::{IROH_ENDPOINT, ToIrohEndpoint};
use crate::actors::streams::ephemeral::{EPHEMERAL_STREAMS, EphemeralStreams, ToEphemeralStreams};
use crate::actors::streams::eventually_consistent::{
    EVENTUALLY_CONSISTENT_STREAMS, EventuallyConsistentStreams, ToEventuallyConsistentStreams,
};
use crate::actors::{generate_actor_namespace, with_namespace, without_namespace};
use crate::args::ApplicationArguments;

pub struct StreamSupervisorState<M>
where
    M: SyncManager<TopicId> + Debug + Send + 'static,
{
    args: ApplicationArguments,
    sync_config: M::Config,
    endpoint: Option<iroh::Endpoint>,
    gossip_actor: ActorRef<ToGossip>,
    gossip_actor_failures: u16,
    eventually_consistent_streams_actor: ActorRef<ToEventuallyConsistentStreams<M>>,
    eventually_consistent_streams_actor_failures: u16,
    ephemeral_streams_actor: ActorRef<ToEphemeralStreams>,
    ephemeral_streams_actor_failures: u16,
}

pub struct StreamSupervisor<M> {
    _phantom: PhantomData<M>,
}

impl<M> Default for StreamSupervisor<M> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<M> ThreadLocalActor for StreamSupervisor<M>
where
    M: SyncManager<TopicId> + Debug + Send + 'static,
{
    type State = StreamSupervisorState<M>;
    type Msg = ();
    type Arguments = (ApplicationArguments, M::Config);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, sync_config) = args;
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

        // Spawn the gossip actor.
        let (gossip_actor, _) = Gossip::<M>::spawn_linked(
            Some(with_namespace(GOSSIP, &actor_namespace)),
            (args.clone(), endpoint.clone()),
            myself.clone().into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        // NOTE: We're registering the gossip protocol in our own iroh endpoint actor outside of
        // the gossip actor itself. All gossip actor tests are not depending on the iroh endpoint
        // actor and would fail otherwise.
        gossip_actor.send_message(ToGossip::RegisterProtocol)?;

        // Spawn the eventually consistent streams actor.
        let (eventually_consistent_streams_actor, _) =
            EventuallyConsistentStreams::<M>::spawn_linked(
                Some(with_namespace(
                    EVENTUALLY_CONSISTENT_STREAMS,
                    &actor_namespace,
                )),
                (args.clone(), gossip_actor.clone(), sync_config.clone()),
                myself.clone().into(),
                args.root_thread_pool.clone(),
            )
            .await?;

        // Spawn the ephemeral streams actor.
        let (ephemeral_streams_actor, _) = EphemeralStreams::spawn_linked(
            Some(with_namespace(EPHEMERAL_STREAMS, &actor_namespace)),
            (args.clone(), gossip_actor.clone()),
            myself.into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        let state = StreamSupervisorState {
            args,
            sync_config,
            endpoint: Some(endpoint),
            gossip_actor,
            gossip_actor_failures: 0,
            eventually_consistent_streams_actor,
            eventually_consistent_streams_actor_failures: 0,
            ephemeral_streams_actor,
            ephemeral_streams_actor_failures: 0,
        };

        Ok(state)
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        trace!("stream supervisor shutting down");

        if let Some(endpoint) = state.endpoint.take() {
            // Make sure the endpoint has all the time it needs to gracefully shut down while other
            // processes might already drop the whole actor.
            tokio::task::spawn(async move {
                endpoint.close().await;
            });
        }

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
                    trace!(
                        "{STREAM_SUPERVISOR} actor: received ready from {} actor",
                        without_namespace(&name)
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                let actor_namespace = generate_actor_namespace(&state.args.public_key);

                if let Some(name) = actor.get_name().as_deref() {
                    if name == with_namespace(GOSSIP, &actor_namespace) {
                        warn!(
                            "{STREAM_SUPERVISOR} actor: {GOSSIP} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the gossip actor.
                        let (gossip_actor, _) = Gossip::<M>::spawn_linked(
                            Some(with_namespace(GOSSIP, &actor_namespace)),
                            (
                                state.args.clone(),
                                state
                                    .endpoint
                                    .as_ref()
                                    .expect("endpoint was initialised when actor started")
                                    .clone(),
                            ),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
                        )
                        .await?;

                        // NOTE: We're registering the gossip protocol in our own iroh endpoint
                        // actor outside of the gossip actor itself. All gossip actor tests are not
                        // depending on the iroh endpoint actor and would fail otherwise.
                        gossip_actor.send_message(ToGossip::RegisterProtocol)?;

                        state.gossip_actor_failures += 1;
                        state.gossip_actor = gossip_actor;
                    } else if name
                        == with_namespace(EVENTUALLY_CONSISTENT_STREAMS, &actor_namespace)
                    {
                        warn!(
                            "{STREAM_SUPERVISOR} actor: {EVENTUALLY_CONSISTENT_STREAMS} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the eventually consistent streams actor.
                        let (eventually_consistent_streams_actor, _) =
                            EventuallyConsistentStreams::<M>::spawn_linked(
                                Some(with_namespace(
                                    EVENTUALLY_CONSISTENT_STREAMS,
                                    &actor_namespace,
                                )),
                                (
                                    state.args.clone(),
                                    state.gossip_actor.clone(),
                                    state.sync_config.clone(),
                                ),
                                myself.clone().into(),
                                state.args.root_thread_pool.clone(),
                            )
                            .await?;

                        state.eventually_consistent_streams_actor_failures += 1;
                        state.eventually_consistent_streams_actor =
                            eventually_consistent_streams_actor;
                    } else if name == with_namespace(EPHEMERAL_STREAMS, &actor_namespace) {
                        warn!(
                            "{STREAM_SUPERVISOR} actor: {EPHEMERAL_STREAMS} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the ephemeral streams actor.
                        let (ephemeral_streams_actor, _) = EphemeralStreams::spawn_linked(
                            Some(with_namespace(EPHEMERAL_STREAMS, &actor_namespace)),
                            (state.args.clone(), state.gossip_actor.clone()),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
                        )
                        .await?;

                        state.ephemeral_streams_actor_failures += 1;
                        state.ephemeral_streams_actor = ephemeral_streams_actor;
                    }
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    trace!(
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
    use ractor::thread_local::ThreadLocalActor;
    use tokio::time::{Duration, sleep};

    use crate::actors::gossip::GOSSIP;
    use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint};
    use crate::actors::streams::ephemeral::EPHEMERAL_STREAMS;
    use crate::actors::streams::eventually_consistent::EVENTUALLY_CONSISTENT_STREAMS;
    use crate::actors::{generate_actor_namespace, with_namespace};
    use crate::test_utils::{DummySyncManager, test_args};

    use super::{STREAM_SUPERVISOR, StreamSupervisor};

    #[tokio::test]
    async fn child_actors_started() {
        let (args, _, sync_config) = test_args();
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
        let (stream_supervisor, stream_supervisor_handle) =
            StreamSupervisor::<DummySyncManager>::spawn(
                Some(with_namespace(STREAM_SUPERVISOR, &actor_namespace)),
                (args.clone(), sync_config),
                args.root_thread_pool.clone(),
            )
            .await
            .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        // Ensure all actors spawned by the stream supervisor are running.
        let gossip_actor = registry::where_is(with_namespace(GOSSIP, &actor_namespace));
        assert!(gossip_actor.is_some());
        assert_eq!(gossip_actor.unwrap().get_status(), ActorStatus::Running);

        let eventually_consistent_streams_actor = registry::where_is(with_namespace(
            EVENTUALLY_CONSISTENT_STREAMS,
            &actor_namespace,
        ));
        assert!(eventually_consistent_streams_actor.is_some());
        assert_eq!(
            eventually_consistent_streams_actor.unwrap().get_status(),
            ActorStatus::Running
        );

        let ephemeral_streams_actor =
            registry::where_is(with_namespace(EPHEMERAL_STREAMS, &actor_namespace));
        assert!(ephemeral_streams_actor.is_some());
        assert_eq!(
            ephemeral_streams_actor.unwrap().get_status(),
            ActorStatus::Running
        );

        stream_supervisor.stop(None);
        stream_supervisor_handle.await.unwrap();
        endpoint_actor.stop(None);
        endpoint_actor_handle.await.unwrap();
    }
}
