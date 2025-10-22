// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use ractor::{Actor, ActorCell, ActorProcessingErr, ActorRef, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::address_book::{ADDRESS_BOOK, AddressBook, ToAddressBook};
use crate::actors::discovery::{DISCOVERY, Discovery, ToDiscovery};
use crate::actors::endpoint::supervisor::{ENDPOINT_SUPERVISOR, EndpointSupervisor};
use crate::actors::events::{Events, ToEvents};
use crate::args::ApplicationArguments;
use crate::store::AddressBookStore;

pub const SUPERVISOR: &str = "net.supervisor";

pub struct SupervisorState<T> {
    application_args: ApplicationArguments,
    events_actor: ActorRef<ToEvents>,
    events_actor_failures: u16,
    address_book_actor: ActorRef<ToAddressBook<T>>,
    address_book_actor_failures: u16,
    discovery_actor: ActorRef<ToDiscovery>,
    discovery_actor_failures: u16,
    endpoint_supervisor: ActorRef<()>,
    endpoint_supervisor_failures: u16,
}

pub struct Supervisor<S, T> {
    store: S,
    _marker: PhantomData<T>,
}

impl<S, T> Supervisor<S, T> {
    pub fn new(store: S) -> Self {
        Self {
            store,
            _marker: PhantomData,
        }
    }
}

impl<S, T> Actor for Supervisor<S, T>
where
    S: AddressBookStore + Clone + Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    type State = SupervisorState<T>;

    type Msg = ();

    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let supervisor: ActorCell = myself.into();

        let (events_actor, _) =
            Actor::spawn_linked(Some("events".to_string()), Events, (), supervisor.clone()).await?;

        let (address_book_actor, _) = Actor::spawn_linked(
            Some(ADDRESS_BOOK.into()),
            AddressBook::new(self.store.clone()),
            (),
            supervisor.clone(),
        )
        .await?;

        let (discovery_actor, _) =
            Actor::spawn_linked(Some(DISCOVERY.into()), Discovery, (), supervisor.clone()).await?;

        let (endpoint_supervisor, _) = Actor::spawn_linked(
            Some(ENDPOINT_SUPERVISOR.into()),
            EndpointSupervisor,
            args.clone(),
            supervisor.clone(),
        )
        .await?;

        let state = SupervisorState {
            application_args: args,
            events_actor,
            events_actor_failures: 0,
            address_book_actor,
            address_book_actor_failures: 0,
            discovery_actor,
            discovery_actor_failures: 0,
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
        let reason = Some("supervisor system is shutting down".to_string());

        // Stop all the actors which are supervised by the supervisor actor.
        state.events_actor.stop(reason.clone());
        state.address_book_actor.stop(reason.clone());
        state.discovery_actor.stop(reason);

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
                    debug!("supervisor actor: received ready from {} actor", name);
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                match actor.get_name().as_deref() {
                    Some("events") => {
                        warn!("supervisor actor: events actor failed: {}", panic_msg);

                        // Respawn the events actor.
                        let (events_actor, _) = Actor::spawn_linked(
                            Some("events".to_string()),
                            Events {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.events_actor_failures += 1;
                        state.events_actor = events_actor;
                    }
                    Some("address book") => {
                        warn!("supervisor actor: address book actor failed: {}", panic_msg);

                        // Respawn the address book actor.
                        let (address_book_actor, _) = Actor::spawn_linked(
                            Some("address book".to_string()),
                            AddressBook::new(self.store.clone()),
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.address_book_actor_failures += 1;
                        state.address_book_actor = address_book_actor;
                    }
                    Some("discovery") => {
                        warn!("supervisor actor: discovery actor failed: {}", panic_msg);

                        // Respawn the discovery actor.
                        let (discovery_actor, _) = Actor::spawn_linked(
                            Some("discovery".to_string()),
                            Discovery {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.discovery_actor_failures += 1;
                        state.discovery_actor = discovery_actor;
                    }
                    _ => (),
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!("supervisor actor: {} actor terminated", name);
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

    use crate::args::ApplicationArguments;
    use crate::store::MemoryStore;

    // Super duper.
    use super::{SUPERVISOR, Supervisor};

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn network_child_actors_are_started() {
        let store = MemoryStore::new();

        let (actor, handle) = Actor::spawn(
            Some(SUPERVISOR.into()),
            Supervisor::<MemoryStore, usize>::new(store),
            ApplicationArguments::default(),
        )
        .await
        .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        actor.stop(None);
        handle.await.unwrap();

        assert!(logs_contain(
            "supervisor actor: received ready from events actor"
        ));
        assert!(logs_contain(
            "supervisor actor: received ready from address_book actor"
        ));
        assert!(logs_contain(
            "supervisor actor: received ready from discovery actor"
        ));

        assert!(!logs_contain("actor failed"));
    }
}
