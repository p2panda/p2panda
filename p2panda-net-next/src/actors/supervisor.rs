// SPDX-License-Identifier: MIT OR Apache-2.0

//! Supervision actor.
//!
//! The root of the entire system supervision tree; it's only role is to spawn and
//! supervise other actors.
//!
//! This supervisor spawns the events and address book actors. It also spawns the endpoint
//! supervisor which is responsible for spawning and monitoring the iroh actors and all others
//! which are reliant on them (e.g. discovery, gossip and sync).
use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::memory::MemoryStore;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tracing::{debug, warn};

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::discovery::{DISCOVERY, Discovery, ToDiscovery};
use crate::actors::endpoint_supervisor::{ENDPOINT_SUPERVISOR, EndpointSupervisor};
use crate::actors::events::{EVENTS, Events, ToEvents};
use crate::actors::iroh::{IROH_ENDPOINT, IrohEndpoint, ToIrohEndpoint};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};
use crate::args::ApplicationArguments;
use crate::{NodeId, NodeInfo};

/// Supervisor actor name.
pub const SUPERVISOR: &str = "net.supervisor";

pub struct SupervisorState<T> {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    store: MemoryStore<ChaCha20Rng, T, NodeId, NodeInfo>,
    thread_pool_1: ThreadLocalActorSpawner,
    events_actor: ActorRef<ToEvents>,
    events_actor_failures: u16,
    address_book_actor: ActorRef<ToAddressBook<T>>,
    address_book_actor_failures: u16,
    endpoint_supervisor: ActorRef<()>,
    endpoint_supervisor_failures: u16,
}

pub struct Supervisor;

impl Actor for Supervisor {
    // @TODO(adz): S and T should be a generic.
    type State = SupervisorState<()>;
    type Msg = ();
    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let actor_namespace = generate_actor_namespace(&args.public_key);

        // @TODO: This is more of a placeholder for proper consideration of how we want to pool the
        // local actors.
        let thread_pool_1 = ThreadLocalActorSpawner::new();

        // Spawn the events actor.
        let (events_actor, _) = Actor::spawn_linked(
            Some(with_namespace(EVENTS, &actor_namespace)),
            Events,
            (),
            myself.clone().into(),
        )
        .await?;

        // @TODO
        let store = MemoryStore::new(ChaCha20Rng::from_os_rng());

        // Spawn the address book actor.
        let (address_book_actor, _) = AddressBook::spawn_linked(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            (store.clone(),),
            myself.clone().into(),
            thread_pool_1.clone(),
        )
        .await?;

        // Spawn the endpoint supervisor.
        let (endpoint_supervisor, _) = Actor::spawn_linked(
            Some(with_namespace(ENDPOINT_SUPERVISOR, &actor_namespace)),
            EndpointSupervisor,
            args.clone(),
            myself.clone().into(),
        )
        .await?;

        let state = SupervisorState {
            actor_namespace,
            args,
            store,
            thread_pool_1,
            events_actor,
            events_actor_failures: 0,
            address_book_actor,
            address_book_actor_failures: 0,
            endpoint_supervisor,
            endpoint_supervisor_failures: 0,
        };

        Ok(state)
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let reason = Some("network system is shutting down".to_string());

        // Stop all the actors which are directly supervised by this actor.
        state.endpoint_supervisor.stop(reason.clone());
        state.events_actor.stop(reason.clone());
        state.address_book_actor.stop(reason.clone());
        state.events_actor.stop(reason.clone());

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
                        "{SUPERVISOR} actor: received ready from {} actor",
                        without_namespace(&name)
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                if let Some(name) = actor.get_name().as_deref() {
                    if name == with_namespace(EVENTS, &state.actor_namespace) {
                        warn!("{SUPERVISOR} actor: {EVENTS} actor failed: {}", panic_msg);

                        // Respawn the events actor.
                        let (events_actor, _) = Actor::spawn_linked(
                            Some(with_namespace(EVENTS, &state.actor_namespace)),
                            Events,
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.events_actor_failures += 1;
                        state.events_actor = events_actor;
                    } else if name == with_namespace(ADDRESS_BOOK, &state.actor_namespace) {
                        warn!(
                            "{SUPERVISOR} actor: {ADDRESS_BOOK} actor failed: {}",
                            panic_msg
                        );

                        let (address_book_actor, _) = AddressBook::spawn_linked(
                            Some(with_namespace(ADDRESS_BOOK, &state.actor_namespace)),
                            (state.store.clone(),),
                            myself.clone().into(),
                            state.thread_pool_1.clone(),
                        )
                        .await?;

                        state.address_book_actor_failures += 1;
                        state.address_book_actor = address_book_actor;
                    } else if name == with_namespace(ENDPOINT_SUPERVISOR, &state.actor_namespace) {
                        warn!(
                            "{SUPERVISOR} actor: {ENDPOINT_SUPERVISOR} actor failed: {}",
                            panic_msg
                        );

                        // Respawn the endpoint supervisor.
                        let (endpoint_supervisor, _) = Actor::spawn_linked(
                            Some(with_namespace(ENDPOINT_SUPERVISOR, &state.actor_namespace)),
                            EndpointSupervisor,
                            state.args.clone(),
                            myself.clone().into(),
                        )
                        .await?;

                        state.endpoint_supervisor_failures += 1;
                        state.endpoint_supervisor = endpoint_supervisor;
                    }
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!(
                        "{SUPERVISOR} actor: {} actor terminated",
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
    use p2panda_core::PrivateKey;
    use ractor::Actor;
    use serial_test::serial;
    use tokio::time::{Duration, sleep};
    use tracing_test::traced_test;

    use crate::actors::address_book::ADDRESS_BOOK;
    use crate::actors::discovery::DISCOVERY;
    use crate::actors::endpoint_supervisor::ENDPOINT_SUPERVISOR;
    use crate::actors::events::EVENTS;
    use crate::actors::{generate_actor_namespace, with_namespace};
    use crate::args::ArgsBuilder;

    use super::{SUPERVISOR, Supervisor};

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn supervisor_child_actors_are_started() {
        let private_key: PrivateKey = Default::default();
        let actor_namespace = generate_actor_namespace(&private_key.public_key());

        let args = ArgsBuilder::new([1; 32]).build();

        let (supervisor_actor, supervisor_actor_handle) = Actor::spawn(
            Some(with_namespace(SUPERVISOR, &actor_namespace)),
            Supervisor,
            args,
        )
        .await
        .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        supervisor_actor.stop(None);
        supervisor_actor_handle.await.unwrap();

        assert!(logs_contain(&format!(
            "{SUPERVISOR} actor: received ready from {EVENTS} actor"
        )));
        assert!(logs_contain(&format!(
            "{SUPERVISOR} actor: received ready from {ADDRESS_BOOK} actor"
        ),));
        assert!(logs_contain(&format!(
            "{SUPERVISOR} actor: received ready from {ENDPOINT_SUPERVISOR} actor"
        )));

        assert!(!logs_contain("actor failed"));
    }
}
