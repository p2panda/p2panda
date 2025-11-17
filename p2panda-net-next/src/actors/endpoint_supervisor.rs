// SPDX-License-Identifier: MIT OR Apache-2.0

//! Endpoint supervisor actor.
//!
//! ```plain
//! - "Endpoint" Supervisor
//!     - "Iroh Endpoint" Actor
//!     - "Discovery Manager" Actor
//!     - "Stream" Supervisor
//! ```
//!
//! This supervisor monitors the health of the iroh endpoint actor, as well as the stream and
//! discovery actors. If the endpoint actor fails, all child actors of the endpoint supervisor are
//! respawned (including the stream and discovery actors); this is necessary because stream and
//! discovery are indirectly reliant on a functioning endpoint actor. If either the stream or
//! discovery actors fail in isolation, they are simply respawned in a one-for-one manner.
use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_discovery::address_book::AddressBookStore;
use p2panda_sync::traits::{Protocol, SyncManager};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, SupervisionEvent};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::TopicId;
use crate::actors::discovery::{DISCOVERY_MANAGER, DiscoveryManager, ToDiscoveryManager};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
use crate::actors::stream_supervisor::{STREAM_SUPERVISOR, StreamSupervisor};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;

/// Endpoint supervisor actor name.
pub const ENDPOINT_SUPERVISOR: &str = "net.endpoint_supervisor";

pub struct EndpointSupervisorState<S, C> {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    store: S,
    sync_config: C,
    iroh_endpoint_actor: ActorRef<ToIrohEndpoint>,
    iroh_endpoint_actor_failures: u16,
    discovery_manager_actor: ActorRef<ToDiscoveryManager<TopicId>>,
    discovery_manager_actor_failures: u16,
    stream_supervisor: ActorRef<()>,
    stream_supervisor_failures: u16,
}

pub struct EndpointSupervisor<S, M> {
    _marker: PhantomData<(S, M)>,
}

impl<S, M> Default for EndpointSupervisor<S, M> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S, M> ThreadLocalActor for EndpointSupervisor<S, M>
where
    S: AddressBookStore<TopicId, NodeId, NodeInfo> + Clone + Debug + Send + Sync + 'static,
    S::Error: StdError + Send + Sync + 'static,
    M: SyncManager<TopicId> + Send + 'static,
    M::Error: StdError + Send + Sync + 'static,
    M::Protocol: Send + Sync + 'static,
    <M::Protocol as Protocol>::Event: Clone + Debug + Send + Sync + 'static,
    <M::Protocol as Protocol>::Error: StdError + Send + Sync + 'static,
{
    type State = EndpointSupervisorState<S, M::Config>;

    type Msg = ();

    type Arguments = (ApplicationArguments, S, M::Config);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, store, sync_config) = args;
        let actor_namespace = generate_actor_namespace(&args.public_key);

        // Spawn the endpoint actor.
        let (iroh_endpoint_actor, _) = IrohEndpoint::spawn_linked(
            Some(with_namespace(IROH_ENDPOINT, &actor_namespace)),
            args.clone(),
            myself.clone().into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        // Spawn the discovery manager actor.
        let (discovery_manager_actor, _) = DiscoveryManager::spawn_linked(
            Some(with_namespace(DISCOVERY_MANAGER, &actor_namespace)),
            (args.clone(), store.clone()),
            myself.clone().into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        // Spawn the stream supervisor.
        let (stream_supervisor, _) = StreamSupervisor::<M>::spawn_linked(
            Some(with_namespace(STREAM_SUPERVISOR, &actor_namespace)),
            (args.clone(), sync_config.clone()),
            myself.clone().into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        Ok(EndpointSupervisorState {
            actor_namespace,
            args,
            store,
            sync_config,
            iroh_endpoint_actor,
            iroh_endpoint_actor_failures: 0,
            discovery_manager_actor,
            discovery_manager_actor_failures: 0,
            stream_supervisor,
            stream_supervisor_failures: 0,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let reason = Some("endpoint supervisor is shutting down".to_string());

        // Stop all the actors which are directly supervised by this actor.
        state.iroh_endpoint_actor.stop(reason.clone());
        state.discovery_manager_actor.stop(reason.clone());
        state.stream_supervisor.stop(reason);

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
                    if name == with_namespace(IROH_ENDPOINT, &state.actor_namespace) {
                        warn!(
                            "{ENDPOINT_SUPERVISOR} actor: {IROH_ENDPOINT} actor failed: {}",
                            panic_msg
                        );

                        // If the endpoint actor fails then we need to:
                        //
                        // 1. Stop the stream supervisor and discovery actors
                        // 2. Respawn the iroh endpoint actor
                        // 3. Respawn the stream supervisor and discovery actors
                        state
                            .stream_supervisor
                            .stop(Some("{IROH_ENDPOINT} actor failed".to_string()));
                        state
                            .discovery_manager_actor
                            .stop(Some("{IROH_ENDPOINT} actor failed".to_string()));

                        // Respawn the iroh endpoint actor.
                        let (iroh_endpoint_actor, _) = IrohEndpoint::spawn_linked(
                            Some(with_namespace(IROH_ENDPOINT, &state.actor_namespace)),
                            state.args.clone(),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
                        )
                        .await?;

                        state.iroh_endpoint_actor_failures += 1;
                        state.iroh_endpoint_actor = iroh_endpoint_actor;

                        // Respawn the discovery manager actor.
                        let (discovery_manager_actor, _) = DiscoveryManager::spawn_linked(
                            Some(with_namespace(DISCOVERY_MANAGER, &state.actor_namespace)),
                            (state.args.clone(), state.store.clone()),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
                        )
                        .await?;

                        state.discovery_manager_actor = discovery_manager_actor;

                        // Respawn the stream supervisor.
                        let (stream_supervisor, _) = StreamSupervisor::<M>::spawn_linked(
                            Some(with_namespace(STREAM_SUPERVISOR, &state.actor_namespace)),
                            (state.args.clone(), state.sync_config.clone()),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
                        )
                        .await?;

                        state.stream_supervisor = stream_supervisor;
                    } else if name == with_namespace(DISCOVERY_MANAGER, &state.actor_namespace) {
                        warn!(
                            "{ENDPOINT_SUPERVISOR} actor: {DISCOVERY_MANAGER} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the discovery actor.
                        let (discovery_manager_actor, _) = DiscoveryManager::spawn_linked(
                            Some(with_namespace(DISCOVERY_MANAGER, &state.actor_namespace)),
                            (state.args.clone(), state.store.clone()),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
                        )
                        .await?;

                        state.discovery_manager_actor_failures += 1;
                        state.discovery_manager_actor = discovery_manager_actor;
                    } else if name == with_namespace(STREAM_SUPERVISOR, &state.actor_namespace) {
                        warn!(
                            "{ENDPOINT_SUPERVISOR} actor: {STREAM_SUPERVISOR} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the stream supervisor.
                        let (stream_supervisor, _) = StreamSupervisor::<M>::spawn_linked(
                            Some(with_namespace(STREAM_SUPERVISOR, &state.actor_namespace)),
                            (state.args.clone(), state.sync_config.clone()),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
                        )
                        .await?;

                        state.stream_supervisor_failures += 1;
                        state.stream_supervisor = stream_supervisor;
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
