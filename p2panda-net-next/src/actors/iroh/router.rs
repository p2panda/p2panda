// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use iroh::endpoint::Incoming as IrohIncoming;
use iroh::protocol::DynProtocolHandler;
use n0_future::join_all;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::warn;

use crate::protocols::ProtocolId;

pub const IROH_ROUTER: &str = "net.iroh.router";

type ProtocolMap = Arc<RwLock<BTreeMap<ProtocolId, Box<dyn DynProtocolHandler>>>>;

pub enum ToIrohRouter {
    RegisterProtocol(ProtocolId, Box<dyn DynProtocolHandler>),
    Incoming(IrohIncoming),
}

pub struct IrohRouterState {
    protocols: ProtocolMap,
    accepted: JoinSet<()>,
}

pub struct IrohRouter;

impl Actor for IrohRouter {
    type State = IrohRouterState;

    type Msg = ToIrohRouter;

    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(IrohRouterState {
            protocols: Arc::default(),
            accepted: JoinSet::new(),
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // We first shutdown the protocol handlers to give them a chance to close connections
        // gracefully.
        let protocols = state.protocols.read().await;
        let handlers = protocols.values().map(|p| p.shutdown());
        join_all(handlers).await;

        // Finally, we abort the remaining accept tasks.
        state.accepted.abort_all();

        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToIrohRouter::RegisterProtocol(protocol_id, protocol_handler) => {
                // @TODO: Mix protocol id with network id.
                let mut protocols = state.protocols.write().await;
                protocols.insert(protocol_id, protocol_handler);
            }
            ToIrohRouter::Incoming(incoming) => {
                let protocols = state.protocols.clone();
                state.accepted.spawn(async move {
                    handle_connection(incoming, protocols).await;
                });
            }
        }

        Ok(())
    }
}

async fn handle_connection(incoming: IrohIncoming, protocols: ProtocolMap) {
    let mut connecting = match incoming.accept() {
        Ok(conn) => conn,
        Err(err) => {
            warn!("ignoring connection: accepting failed: {err:#}");
            return;
        }
    };

    let alpn = match connecting.alpn().await {
        Ok(alpn) => alpn,
        Err(err) => {
            warn!("ignoring connection: invalid handshake: {err:#}");
            return;
        }
    };

    let protocols = protocols.read().await;

    let Some(handler) = protocols.get(&alpn) else {
        warn!("ignoring connection: unsupported alpn protocol");
        return;
    };

    match handler.on_accepting(connecting).await {
        Ok(connection) => {
            if let Err(err) = handler.accept(connection).await {
                warn!("handling incoming connection ended with error: {err}");
            }
        }
        Err(err) => {
            warn!("handling incoming connecting ended with error: {err}");
        }
    }
}
