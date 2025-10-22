// SPDX-License-Identifier: MIT OR Apache-2.0

use iroh::protocol::ProtocolHandler;
use ractor::{Actor, ActorCell, ActorProcessingErr, ActorRef, RpcReplyPort, call, registry};
use thiserror::Error;

use crate::NodeId;
use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::endpoint::connection::Connection;
use crate::actors::endpoint::iroh::{IROH_TRANSPORT, ToIroh};
use crate::protocols::ProtocolId;

pub const CONNECTION_MANAGER: &str = "net.endpoint.connectionmanager";

pub enum ToConnectionManager {
    Connect(
        NodeId,
        ProtocolId,
        RpcReplyPort<Result<ActorRef<()>, ConnectionManagerError>>,
    ),
}

pub struct ConnectionManager;

impl Actor for ConnectionManager {
    type State = ();

    type Msg = ToConnectionManager;

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToConnectionManager::Connect(node_id, protocol_id, reply) => {
                let Some(actor) = registry::where_is(ADDRESS_BOOK.into()) else {
                    // @TODO: Address book is not running.
                    return Ok(());
                };

                // @TODO: Bring T here.
                let address_book_actor: ActorRef<ToAddressBook<()>> = actor.into();
                let Ok(node_info) = call!(address_book_actor, ToAddressBook::NodeInfo, node_id)
                else {
                    // @TODO: call failed?
                    return Ok(());
                };

                let Some(node_info) = node_info else {
                    // @TODO: No info for that node id.
                    return Ok(());
                };

                let Ok(node_addr) = iroh::NodeAddr::try_from(node_info) else {
                    // @TODO: No iroh address for that node id.
                    return Ok(());
                };

                let Some(actor) = registry::where_is(IROH_TRANSPORT.into()) else {
                    // @TODO: Iroh actor is not running.
                    return Ok(());
                };

                let iroh_actor: ActorRef<ToIroh> = actor.into();
                let Ok(result) = call!(iroh_actor, ToIroh::Connect, node_addr, protocol_id) else {
                    // @TODO: call failed?
                    return Ok(());
                };

                let Ok(connecting) = result else {
                    // @TODO: connection attempt failed.
                    return Ok(());
                };

                let (connection_actor, _) =
                    Actor::spawn_linked(None, Connection, (connecting,), myself.into()).await?;

                let _ = reply.send(Ok(connection_actor));
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct IncomingConnectionHandler<T> {
    actor_ref: ActorRef<T>,
}

impl<T> ProtocolHandler for IncomingConnectionHandler<T>
where
    T: std::fmt::Debug + 'static,
{
    async fn accept(
        &self,
        _connection: iroh::endpoint::Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        todo!()
    }
}

impl<T> IncomingConnectionHandler<T> {
    pub fn new(actor_cell: ActorCell) -> Self {
        Self {
            actor_ref: actor_cell.into(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConnectionManagerError {}
