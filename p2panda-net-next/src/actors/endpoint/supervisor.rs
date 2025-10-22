// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::{Actor, ActorCell, ActorProcessingErr, ActorRef, SupervisionEvent};

use crate::actors::endpoint::connection_manager::{
    CONNECTION_MANAGER, ConnectionManager, ToConnectionManager,
};
use crate::actors::endpoint::iroh::{IROH_TRANSPORT, IrohTransport, ToIroh};
use crate::actors::endpoint::router::{ROUTER, Router, ToRouter};
use crate::args::ApplicationArguments;

pub const ENDPOINT_SUPERVISOR: &str = "net.endpoint.supervisor";

pub struct EndpointSupervisorState {
    application_args: ApplicationArguments,
    iroh_actor: ActorRef<ToIroh>,
    iroh_actor_failures: usize,
    router_actor: ActorRef<ToRouter>,
    router_actor_failures: usize,
    connection_manager_actor: ActorRef<ToConnectionManager>,
    connection_manager_actor_failures: usize,
}

pub struct EndpointSupervisor;

impl Actor for EndpointSupervisor {
    type State = EndpointSupervisorState;

    type Msg = ();

    type Arguments = ApplicationArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let supervisor: ActorCell = myself.into();

        let (router_actor, _) =
            Actor::spawn_linked(Some(ROUTER.into()), Router, (), supervisor.clone()).await?;

        let (iroh_actor, _) = Actor::spawn_linked(
            Some(IROH_TRANSPORT.into()),
            IrohTransport,
            args.clone(),
            supervisor.clone(),
        )
        .await?;

        let (connection_manager_actor, _) = Actor::spawn_linked(
            Some(CONNECTION_MANAGER.into()),
            ConnectionManager,
            (),
            supervisor.clone(),
        )
        .await?;

        Ok(EndpointSupervisorState {
            application_args: args,
            iroh_actor,
            iroh_actor_failures: 0,
            router_actor,
            router_actor_failures: 0,
            connection_manager_actor,
            connection_manager_actor_failures: 0,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let reason = Some("endpoint supervisor is shutting down".to_string());
        state.iroh_actor.stop(reason.clone());
        state.router_actor.stop(reason.clone());
        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match event {
            SupervisionEvent::ActorFailed(actor, _error) => match actor.get_name().as_deref() {
                Some(IROH_TRANSPORT) => {
                    let (iroh_actor, _) = Actor::spawn_linked(
                        Some(IROH_TRANSPORT.into()),
                        IrohTransport,
                        state.application_args.clone(),
                        myself.into(),
                    )
                    .await?;
                    state.iroh_actor_failures += 1;
                    state.iroh_actor = iroh_actor;
                }
                Some(ROUTER) => {
                    let (router_actor, _) =
                        Actor::spawn_linked(Some(ROUTER.into()), Router, (), myself.into()).await?;
                    state.router_actor_failures += 1;
                    state.router_actor = router_actor;
                }
                Some(CONNECTION_MANAGER) => {
                    let (connection_manager_actor, _) = Actor::spawn_linked(
                        Some(CONNECTION_MANAGER.into()),
                        ConnectionManager,
                        (),
                        myself.into(),
                    )
                    .await?;
                    state.connection_manager_actor_failures += 1;
                    state.connection_manager_actor = connection_manager_actor;
                }
                _ => unreachable!("actor is not managed by this supervisor"),
            },
            _ => (),
        }

        Ok(())
    }
}
