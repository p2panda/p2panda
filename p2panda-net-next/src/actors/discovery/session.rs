// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::naive::{NaiveDiscoveryMessage, NaiveDiscoveryProtocol};
use p2panda_discovery::traits::DiscoveryProtocol as _;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use serde::{Deserialize, Serialize};
use tracing::{instrument, trace};

use crate::actors::ActorNamespace;
use crate::actors::discovery::walker::ToDiscoveryWalker;
use crate::actors::discovery::{DISCOVERY_PROTOCOL_ID, SubscriptionInfo, ToDiscoveryManager};
use crate::actors::iroh::connect;
use crate::addrs::{NodeId, NodeInfo};
use crate::cbor::{into_cbor_sink, into_cbor_stream};

/// Actor name prefix for a session.
pub const DISCOVERY_SESSION: &str = "net.discovery.session";

pub type DiscoverySessionId = u64;

#[derive(Debug)]
pub struct DiscoverySession<S, T> {
    _marker: PhantomData<(S, T)>,
}

impl<S, T> Default for DiscoverySession<S, T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

pub enum ToDiscoverySession<S, T> {
    Initiate(DiscoverySessionArguments<S, T>),
}

pub type DiscoverySessionArguments<S, T> = (
    ActorNamespace,
    DiscoverySessionId,
    NodeId,
    S,
    ActorRef<ToDiscoveryManager<T>>,
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

impl<S, T> ThreadLocalActor for DiscoverySession<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Debug + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
{
    type State = ();

    type Msg = ToDiscoverySession<S, T>;

    type Arguments = DiscoverySessionArguments<S, T>;

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
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let ToDiscoverySession::Initiate(args) = message;
        let (actor_namespace, session_id, remote_node_id, store, manager_ref, args) = args;
        let role = args.role();

        let (tx, rx) = match args {
            DiscoverySessionRole::Connect => {
                trace!("try to connect");
                // Try to establish a direct connection with this node.
                let connection =
                    connect::<T>(remote_node_id, DISCOVERY_PROTOCOL_ID, actor_namespace).await?;
                trace!("lala");
                connection.open_bi().await?
            }
            DiscoverySessionRole::Accept { connection } => connection.accept_bi().await?,
        };

        trace!("connect established");

        // Establish bi-directional QUIC stream as part of the direct connection and use CBOR
        // encoding for message framing.
        let mut tx = into_cbor_sink::<NaiveDiscoveryMessage<T, NodeId, NodeInfo>, _>(tx);
        let mut rx = into_cbor_stream::<NaiveDiscoveryMessage<T, NodeId, NodeInfo>, _>(rx);

        // Run the discovery protocol.
        // @TODO: Have a timeout to cancel session if it's running overtime.
        let protocol = NaiveDiscoveryProtocol::<S, _, T, NodeId, NodeInfo>::new(
            store,
            SubscriptionInfo::<T>::new(),
            remote_node_id,
        );
        let result = match role {
            Role::Alice => protocol.alice(&mut tx, &mut rx).await?,
            Role::Bob => protocol.bob(&mut tx, &mut rx).await?,
        };

        // Inform manager about our results.
        let _ = manager_ref.send_message(ToDiscoveryManager::OnSuccess(session_id, result));

        // Stop this actor for good.
        myself.stop(None);

        Ok(())
    }
}
