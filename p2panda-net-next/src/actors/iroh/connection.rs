// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort};
use thiserror::Error;
use tokio::task::JoinHandle;

use crate::actors::iroh::endpoint::ProtocolMap;
use crate::protocols::ProtocolId;

pub type ConnectionReplyPort =
    RpcReplyPort<Result<iroh::endpoint::Connection, ConnectionActorError>>;

#[allow(clippy::large_enum_variant)]
pub enum IrohConnectionArgs {
    Connect {
        endpoint: iroh::endpoint::Endpoint,
        node_addr: iroh::EndpointAddr,
        alpn: ProtocolId,
        reply: ConnectionReplyPort,
    },
    Accept {
        incoming: iroh::endpoint::Incoming,
        protocols: ProtocolMap,
    },
}

pub struct IrohConnectionState {
    handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
pub struct IrohConnection;

impl ThreadLocalActor for IrohConnection {
    type State = IrohConnectionState;

    type Msg = ();

    type Arguments = IrohConnectionArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let handle = match args {
            IrohConnectionArgs::Connect {
                endpoint,
                node_addr,
                alpn,
                reply,
            } => match endpoint
                .connect(node_addr, &alpn)
                .await
                .map_err(|err| ConnectionActorError::Iroh(err.into()))
            {
                Ok(connection) => {
                    // Give connection object to the caller and stop actor, we're done here.
                    let _ = reply.send(Ok(connection));
                    myself.stop(None);
                    None
                }
                Err(err) => {
                    // Inform caller about what went wrong and shut down actor with a failure.
                    // Since the error types do not implement `Clone` we're helping ourselves with
                    // an own type holding the string representation.
                    let reason = err.to_string();
                    let _ = reply.send(Err(err));
                    return Err(ConnectionActorError::ConnectionAttemptFailed(reason).into());
                }
            },
            IrohConnectionArgs::Accept {
                incoming,
                protocols,
            } => {
                // Check incoming request and establish connection when valid ALPN.
                let (connection, alpn) = accept_incoming(incoming, &protocols).await?;

                // Spawn a task which executes the actual protocol. As soon as this has finished
                // we're telling the actor to finally shut down.
                let handle = tokio::spawn(async move {
                    let protocols = protocols.read().await;
                    let Some(protocol_handler) = protocols.get(&alpn) else {
                        unreachable!("already checked in accept_incoming if this alpn exists");
                    };

                    let _ = protocol_handler.accept(connection).await;
                    myself.stop(None);
                });

                Some(handle)
            }
        };

        Ok(IrohConnectionState { handle })
    }
}

async fn accept_incoming(
    incoming: iroh::endpoint::Incoming,
    protocols: &ProtocolMap,
) -> Result<(iroh::endpoint::Connection, ProtocolId), ConnectionActorError> {
    // Accept incoming request.
    let mut connecting = incoming
        .accept()
        .map_err(|err| ConnectionActorError::Iroh(err.into()))?;

    // Check if we're supporting this ALPN.
    let alpn = connecting
        .alpn()
        .await
        .map_err(|err| ConnectionActorError::Iroh(err.into()))?;
    let protocols = protocols.read().await;
    let Some(protocol_handler) = protocols.get(&alpn) else {
        return Err(ConnectionActorError::InvalidAlpnHandshake(alpn));
    };

    // Establish connection.
    let connection = protocol_handler
        .on_accepting(connecting)
        .await
        .map_err(|err| ConnectionActorError::Iroh(err.into()))?;
    Ok((connection, alpn))
}

#[derive(Debug, Error)]
pub enum IrohError {
    #[error(transparent)]
    Connect(#[from] iroh::endpoint::ConnectError),

    #[error(transparent)]
    Connection(#[from] iroh::endpoint::ConnectionError),

    #[error(transparent)]
    Alpn(#[from] iroh::endpoint::AlpnError),

    #[error(transparent)]
    Accept(#[from] iroh::protocol::AcceptError),
}

#[derive(Debug, Error)]
pub enum ConnectionActorError {
    #[error("{0}")]
    Iroh(IrohError),

    #[error("remote node tried to connect with unknown alpn")]
    InvalidAlpnHandshake(Vec<u8>),

    #[error("{0}")]
    ConnectionAttemptFailed(String),
}
