// SPDX-License-Identifier: MIT OR Apache-2.0

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort};

use crate::actors::iroh::endpoint::ProtocolMap;
use crate::protocols::ProtocolId;

pub type ConnectionReplyPort =
    RpcReplyPort<Result<iroh::endpoint::Connection, iroh::endpoint::ConnectWithOptsError>>;

#[allow(clippy::large_enum_variant)]
pub enum IrohConnectionArgs {
    Connect {
        endpoint: iroh::endpoint::Endpoint,
        node_addr: iroh::NodeAddr,
        alpn: ProtocolId,
        reply: ConnectionReplyPort,
    },
    Accept {
        incoming: iroh::endpoint::Incoming,
        protocols: ProtocolMap,
    },
}

pub enum ToIrohConnection {}

pub struct IrohConnectionState {}

#[derive(Default)]
pub struct IrohConnection;

impl ThreadLocalActor for IrohConnection {
    type State = IrohConnectionState;

    type Msg = ToIrohConnection;

    type Arguments = IrohConnectionArgs;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(IrohConnectionState {})
    }
}
