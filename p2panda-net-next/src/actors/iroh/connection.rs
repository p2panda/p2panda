// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort};
use thiserror::Error;
use tracing::field::Empty;
use tracing::{Instrument, debug, info_span, warn};

use crate::NodeId;
use crate::actors::iroh::endpoint::ProtocolMap;
use crate::protocols::ProtocolId;
use crate::utils::ShortFormat;

pub type ConnectionReplyPort =
    RpcReplyPort<Result<iroh::endpoint::Connection, ConnectionActorError>>;

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum IrohConnectionArgs {
    Connect {
        endpoint: iroh::endpoint::Endpoint,
        endpoint_addr: iroh::EndpointAddr,
        alpn: ProtocolId,
        reply: ConnectionReplyPort,
    },
    Accept {
        incoming: iroh::endpoint::Incoming,
        protocols: ProtocolMap,
    },
}

pub enum ToIrohConnection {
    EstablishConnection(NodeId, IrohConnectionArgs),
}

#[derive(Default)]
pub struct IrohConnection;

impl ThreadLocalActor for IrohConnection {
    type State = ();

    type Msg = ToIrohConnection;

    type Arguments = (NodeId, IrohConnectionArgs);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Kick-off connection establishment directly after start.
        let (node_id, args) = args;
        myself.send_message(ToIrohConnection::EstablishConnection(node_id, args))?;
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToIrohConnection::EstablishConnection(node_id, args) => {
                let span =
                    info_span!("connection", me=%node_id.fmt_short(), remote=Empty, alpn=Empty);

                // This blocks for a while but that's okay since we're inside an independent actor.
                establish_connection(args).instrument(span).await?;

                // If something failed this actor already terminated, propagating the error to the
                // parent actor, if we're done here after a successful connection attempt, we stop
                // ourselves.
                myself.stop(None);
            }
        }
        Ok(())
    }
}

async fn establish_connection(args: IrohConnectionArgs) -> Result<(), ConnectionActorError> {
    match args {
        IrohConnectionArgs::Connect {
            endpoint,
            endpoint_addr,
            alpn,
            reply,
        } => {
            tracing::Span::current().record("alpn", alpn.fmt_short());
            debug!("try to initialise connection");
            match endpoint
                .connect(endpoint_addr, &alpn)
                .await
                .map_err(|err| ConnectionActorError::Iroh(err.into()))
            {
                Ok(connection) => {
                    debug!("successfully initiated connection");
                    // Give connection object to the caller and stop actor, we're done here.
                    let _ = reply.send(Ok(connection));
                }
                Err(err) => {
                    warn!("connection establishment failed: {err:#}");
                    // Inform caller about what went wrong and shut down actor with a failure.
                    // Since the error types do not implement `Clone` we're helping ourselves with
                    // an own type holding the string representation.
                    let reason = err.to_string();
                    let _ = reply.send(Err(err));
                    return Err(ConnectionActorError::ConnectionAttemptFailed(reason));
                }
            }
        }
        IrohConnectionArgs::Accept {
            incoming,
            protocols,
        } => {
            debug!("accept incoming connection");
            // Accept incoming request.
            let mut accepting = match incoming.accept() {
                Ok(accepting) => accepting,
                Err(err) => {
                    warn!("ignoring connection: accepting failed: {err:#}");
                    return Err(ConnectionActorError::Iroh(err.into()));
                }
            };

            // Check if we're supporting this ALPN.
            let alpn = match accepting.alpn().await {
                Ok(alpn) => alpn,
                Err(err) => {
                    warn!("ignoring connection: invalid handshake: {err:#}");
                    return Err(ConnectionActorError::Iroh(err.into()));
                }
            };
            tracing::Span::current().record("alpn", alpn.fmt_short());
            let protocols = protocols.read().await;
            let Some(protocol_handler) = protocols.get(&alpn) else {
                warn!("ignoring connection: unsupported alpn protocol");
                return Err(ConnectionActorError::InvalidAlpnHandshake(alpn));
            };

            // Establish connection.
            let connection = match protocol_handler.on_accepting(accepting).await {
                Ok(connection) => connection,
                Err(err) => {
                    warn!("accepting incoming connection ended with error: {err}");
                    return Err(ConnectionActorError::Iroh(err.into()));
                }
            };
            tracing::Span::current().record(
                "remote",
                tracing::field::display(connection.remote_id().fmt_short()),
            );
            debug!("successfully accepted connection");

            // Pass over connection to handler, ignore any errors here as this is nothing we need
            // to be aware of anymore, this is the end of this actor.
            if let Err(err) = protocol_handler.accept(connection).await {
                warn!("errrorr: {err:#?}");
            }

            debug!("end here");
        }
    }

    Ok(())
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
