// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::naive::{NaiveDiscoveryMessage, NaiveDiscoveryProtocol};
use p2panda_discovery::traits::DiscoveryProtocol as _;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

use crate::actors::ActorNamespace;
use crate::actors::discovery::{DISCOVERY_PROTOCOL_ID, LocalTopicsProvider, ToDiscoveryManager};
use crate::actors::iroh::connect;
use crate::addrs::{NodeId, NodeInfo};
use crate::cbor::{into_cbor_sink, into_cbor_stream};

/// Actor name prefix for a session.
pub const DISCOVERY_SESSION: &str = "net.discovery.session";

pub type DiscoverySessionId = u64;

#[derive(Debug)]
pub struct DiscoverySession<S> {
    _marker: PhantomData<S>,
}

impl<S> Default for DiscoverySession<S> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

pub enum ToDiscoverySession<S> {
    Initiate(DiscoverySessionArguments<S>),
}

pub type DiscoverySessionArguments<S> = (
    ActorNamespace,
    DiscoverySessionId,
    NodeId,
    S,
    ActorRef<ToDiscoveryManager>,
    DiscoverySessionRole,
);

#[derive(Debug)]
pub enum DiscoverySessionRole {
    Connect,
    Accept {
        connection: iroh::endpoint::Connection,
    },
}

impl DiscoverySessionRole {
    fn role(&self) -> Role {
        match self {
            DiscoverySessionRole::Connect => Role::Alice,
            DiscoverySessionRole::Accept { .. } => Role::Bob,
        }
    }
}

enum Role {
    Alice,
    Bob,
}

impl<S> ThreadLocalActor for DiscoverySession<S>
where
    S: AddressBookStore<NodeId, NodeInfo> + Clone + Debug + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
{
    type State = ();

    type Msg = ToDiscoverySession<S>;

    type Arguments = DiscoverySessionArguments<S>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        myself.send_message(ToDiscoverySession::Initiate(args))?;
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let ToDiscoverySession::Initiate(args) = message;
        let (actor_namespace, session_id, remote_node_id, store, manager_ref, args) = args;
        let role = args.role();

        let (connection, tx, rx) = match args {
            DiscoverySessionRole::Connect => {
                // Try to establish a direct connection with this node.
                let connection = connect(
                    remote_node_id,
                    DISCOVERY_PROTOCOL_ID,
                    actor_namespace.clone(),
                )
                .await?;
                let (tx, rx) = connection.open_bi().await?;
                (connection, tx, rx)
            }
            DiscoverySessionRole::Accept { connection } => {
                let (tx, rx) = connection.accept_bi().await?;
                (connection, tx, rx)
            }
        };

        // Establish bi-directional QUIC stream as part of the direct connection and use CBOR
        // encoding for message framing.
        let mut tx = into_cbor_sink::<NaiveDiscoveryMessage<NodeId, NodeInfo>, _>(tx);
        let mut rx = into_cbor_stream::<NaiveDiscoveryMessage<NodeId, NodeInfo>, _>(rx);

        // Run the discovery protocol.
        // @TODO: Have a timeout to cancel session if it's running overtime.
        let protocol = NaiveDiscoveryProtocol::<S, _, NodeId, NodeInfo>::new(
            store,
            LocalTopicsProvider::new(actor_namespace),
            remote_node_id,
        );
        let result = match role {
            Role::Alice => {
                let result = protocol.alice(&mut tx, &mut rx).await?;
                connection.close(0u32.into(), b"done");
                result
            }
            Role::Bob => {
                let result = protocol.bob(&mut tx, &mut rx).await?;
                connection.closed().await;
                result
            }
        };

        // Inform manager about our results.
        let _ = manager_ref.send_message(ToDiscoveryManager::OnSuccess(session_id, result));

        // Stop this actor for good.
        myself.stop(None);

        Ok(())
    }
}
