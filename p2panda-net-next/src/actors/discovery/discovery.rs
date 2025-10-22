// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use iroh::protocol::{AcceptError, ProtocolHandler};
use p2panda_core::PrivateKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};

use crate::actors::discovery::DiscoverySession;
use crate::actors::{connect, register_protocol};

pub const DISCOVERY: &str = "net.discovery";

pub const DISCOVERY_PROTOCOL_ID: &[u8] = b"p2panda/discovery/v1";

pub struct DiscoveryState {}

pub struct Discovery<T> {
    _marker: PhantomData<T>,
}

impl<T> Default for Discovery<T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T> Actor for Discovery<T>
where
    T: Send + Sync + 'static,
{
    type State = DiscoveryState;

    type Msg = ();

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

        // @TODO: Example: Create a connection
        let node_id = PrivateKey::new().public_key();

        let connecting = connect::<T>(node_id, DISCOVERY_PROTOCOL_ID).await?;
        // @TODO: This should not be part of the actor as it will take too much time.
        let connection = connecting.await?;

        myself
            .spawn_linked(None, DiscoverySession, (connection,))
            .await
            .map_err(|err| AcceptError::from_err(err))?;

        Ok(DiscoveryState {})
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: ractor::SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(_actor_cell) => {
                // @TODO
                // Discovery session started
            }
            SupervisionEvent::ActorTerminated(_actor_cell, _boxed_state, _) => {
                // @TODO
                // Discovery session finished
            }
            SupervisionEvent::ActorFailed(_actor_cell, _error) => {
                // @TODO
                // Discovery session failed
            }
            _ => (),
        }

        Ok(())
    }
}

#[derive(Debug)]
struct DiscoveryProtocolHandler(ActorRef<()>);

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
