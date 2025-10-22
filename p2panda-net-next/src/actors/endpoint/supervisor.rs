// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::{Actor, ActorCell, ActorProcessingErr, ActorRef, SupervisionEvent};

use crate::actors::endpoint::connection_manager::{
    CONNECTION_MANAGER, ConnectionManager, ToConnectionManager,
};
use crate::actors::endpoint::iroh::{IROH_TRANSPORT, IrohTransport, ToIroh};
use crate::actors::endpoint::router::{IROH_ROUTER, IrohRouter, ToIrohRouter};
use crate::args::ApplicationArguments;

pub const ENDPOINT_SUPERVISOR: &str = "net.endpoint.supervisor";

pub struct EndpointSupervisorState {
    application_args: ApplicationArguments,
    iroh_actor: ActorRef<ToIroh>,
    iroh_actor_failures: usize,
    iroh_router_actor: ActorRef<ToIrohRouter>,
    iroh_router_actor_failures: usize,
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

        let (iroh_router_actor, _) =
            Actor::spawn_linked(Some(IROH_ROUTER.into()), IrohRouter, (), supervisor.clone())
                .await?;

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
            iroh_router_actor,
            iroh_router_actor_failures: 0,
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
        state.iroh_router_actor.stop(reason.clone());
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
                Some(IROH_ROUTER) => {
                    let (iroh_router_actor, _) = Actor::spawn_linked(
                        Some(IROH_ROUTER.into()),
                        IrohRouter,
                        (),
                        myself.into(),
                    )
                    .await?;
                    state.iroh_router_actor_failures += 1;
                    state.iroh_router_actor = iroh_router_actor;
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
