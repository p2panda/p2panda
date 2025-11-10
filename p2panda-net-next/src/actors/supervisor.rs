// SPDX-License-Identifier: MIT OR Apache-2.0

//! Supervision actor.
//!
//! The root of the entire system supervision tree; it's only role is to spawn and supervise other
//! actors.
//!
//! ```plain
//! - "Root" Supervisor
//!     - "Events" Actor
//!     - "Address Book" Actor
//!     - "Endpoint" Supervisor
//! ```
//!
//! This supervisor spawns the events and address book actors. It also spawns the endpoint
//! supervisor which is responsible for spawning and monitoring the iroh actors and all others
//! which are reliant on them (e.g. discovery, gossip and sync).
use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::address_book::memory::MemoryStore;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, SupervisionEvent};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::endpoint_supervisor::{ENDPOINT_SUPERVISOR, EndpointSupervisor};
use crate::actors::events::{EVENTS, Events, ToEvents};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace, without_namespace};
use crate::args::ApplicationArguments;
use crate::{NodeId, NodeInfo};

/// Supervisor actor name.
pub const SUPERVISOR: &str = "net.supervisor";

pub struct SupervisorState<S, T> {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    store: S,
    events_actor: ActorRef<ToEvents>,
    events_actor_failures: u16,
    address_book_actor: ActorRef<ToAddressBook<T>>,
    address_book_actor_failures: u16,
    endpoint_supervisor: ActorRef<()>,
    endpoint_supervisor_failures: u16,
}

pub struct Supervisor<S, T> {
    _marker: PhantomData<(S, T)>,
}

impl<S, T> Default for Supervisor<S, T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S, T> ThreadLocalActor for Supervisor<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Debug + Send + Sync + 'static,
    S::Error: StdError + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    type State = SupervisorState<S, T>;

    type Msg = ();

    type Arguments = (ApplicationArguments, S);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, store) = args;
        let actor_namespace = generate_actor_namespace(&args.public_key);

        // Spawn the events actor.
        let (events_actor, _) = Events::spawn_linked(
            Some(with_namespace(EVENTS, &actor_namespace)),
            (),
            myself.clone().into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        // Spawn the address book actor.
        let (address_book_actor, _) = AddressBook::spawn_linked(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            (store.clone(),),
            myself.clone().into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        // Spawn the endpoint supervisor.
        let (endpoint_supervisor, _) = EndpointSupervisor::spawn_linked(
            Some(with_namespace(ENDPOINT_SUPERVISOR, &actor_namespace)),
            (args.clone(), store.clone()),
            myself.clone().into(),
            args.root_thread_pool.clone(),
        )
        .await?;

        Ok(SupervisorState {
            actor_namespace,
            args,
            store,
            events_actor,
            events_actor_failures: 0,
            address_book_actor,
            address_book_actor_failures: 0,
            endpoint_supervisor,
            endpoint_supervisor_failures: 0,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let reason = Some("network system is shutting down".to_string());

        // Stop all the actors which are directly supervised by this actor.
        state.events_actor.stop(reason.clone());
        state.address_book_actor.stop(reason.clone());
        state.endpoint_supervisor.stop(reason.clone());

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

                        let (events_actor, _) = Events::spawn_linked(
                            Some(with_namespace(EVENTS, &state.actor_namespace)),
                            (),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
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
                            state.args.root_thread_pool.clone(),
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
                        let (endpoint_supervisor, _) = EndpointSupervisor::spawn_linked(
                            Some(with_namespace(ENDPOINT_SUPERVISOR, &state.actor_namespace)),
                            (state.args.clone(), state.store.clone()),
                            myself.clone().into(),
                            state.args.root_thread_pool.clone(),
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
    use ractor::actor::actor_cell::ActorStatus;
    use ractor::registry;
    use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
    use tokio::time::{Duration, sleep};

    use crate::actors::address_book::ADDRESS_BOOK;
    use crate::actors::endpoint_supervisor::ENDPOINT_SUPERVISOR;
    use crate::actors::events::EVENTS;
    use crate::actors::{generate_actor_namespace, with_namespace};
    use crate::args::ArgsBuilder;
    use crate::args::test_utils::test_args;

    use super::{SUPERVISOR, Supervisor};

    #[tokio::test]
    async fn child_actors_started() {
        let (args, store) = test_args();
        let actor_namespace = generate_actor_namespace(&args.public_key);

        let (supervisor_actor, supervisor_actor_handle) = Supervisor::spawn(
            Some(with_namespace(SUPERVISOR, &actor_namespace)),
            (args.clone(), store),
            args.root_thread_pool,
        )
        .await
        .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        // Ensure all actors spawned directly by the supervisor are running.
        let events_actor = registry::where_is(with_namespace(EVENTS, &actor_namespace));
        assert!(events_actor.is_some());
        assert_eq!(events_actor.unwrap().get_status(), ActorStatus::Running);

        let address_book_actor = registry::where_is(with_namespace(ADDRESS_BOOK, &actor_namespace));
        assert!(address_book_actor.is_some());
        assert_eq!(
            address_book_actor.unwrap().get_status(),
            ActorStatus::Running
        );

        let endpoint_supervisor =
            registry::where_is(with_namespace(ENDPOINT_SUPERVISOR, &actor_namespace));
        assert!(endpoint_supervisor.is_some());
        assert_eq!(
            endpoint_supervisor.unwrap().get_status(),
            ActorStatus::Running
        );

        supervisor_actor.stop(None);
        supervisor_actor_handle.await.unwrap();
    }
}
