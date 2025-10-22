// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::protocol::{AcceptError, ProtocolHandler};
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};

use crate::actors::discovery::DiscoverySession;
use crate::actors::endpoint::router::register_protocol;

pub const DISCOVERY: &str = "net.discovery";

pub const DISCOVERY_PROTOCOL_ID: &[u8] = b"p2panda/discovery/v1";

pub enum ToDiscovery {}

pub struct DiscoveryState {}

pub struct Discovery;

impl Actor for Discovery {
    type State = DiscoveryState;

    type Msg = ToDiscovery;

    type Arguments = ();

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Register protocol handler to accept incoming "discovery protocol" connection requests.
        register_protocol(
            DISCOVERY_PROTOCOL_ID,
            DiscoveryProtocolHandler(myself.clone()),
        )?;

        Ok(DiscoveryState {})
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: ractor::SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(actor_cell) => {
                // @TODO
                // Discovery session started
            }
            SupervisionEvent::ActorTerminated(actor_cell, boxed_state, _) => {
                // @TODO
                // Discovery session finished
            }
            SupervisionEvent::ActorFailed(actor_cell, error) => {
                // @TODO
                // Discovery session failed
            }
            _ => (),
        }

        Ok(())
    }
}

#[derive(Debug)]
struct DiscoveryProtocolHandler(ActorRef<ToDiscovery>);

impl ProtocolHandler for DiscoveryProtocolHandler {
    async fn accept(
        &self,
        connection: iroh::endpoint::Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        let (_connection_actor, handle) = self
            .0
            .spawn_linked(None, DiscoverySession, (connection,))
            .await
            .map_err(|err| AcceptError::from_err(err))?;

        // Wait until discovery session ended.
        handle.await.map_err(|err| AcceptError::from_err(err))?;

        Ok(())
    }
}
