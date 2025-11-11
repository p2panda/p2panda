// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::naive::{NaiveDiscoveryMessage, NaiveDiscoveryProtocol};
use p2panda_discovery::traits::{DiscoveryProtocol as _, SubscriptionInfo as _};
use ractor::thread_local::ThreadLocalActor;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::actors::ActorNamespace;
use crate::actors::discovery::manager::SubscriptionInfo;
use crate::actors::discovery::walker::ToDiscoveryWalker;
use crate::actors::discovery::{DISCOVERY_PROTOCOL_ID, ToDiscoveryManager};
use crate::actors::iroh::connect;
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::cbor::{CborCodec, into_cbor_sink, into_cbor_stream};

/// Actor name prefix for a session.
pub const DISCOVERY_SESSION: &str = "net.discovery.session";

pub type DiscoverySessionId = u64;

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

pub enum DiscoverySessionArguments<T> {
    Connect {
        walker_ref: ActorRef<ToDiscoveryWalker<T>>,
    },
    Accept {
        connection: iroh::endpoint::Connection,
    },
}

impl<T> DiscoverySessionArguments<T> {
    pub fn role(&self) -> DiscoverySessionRole {
        match self {
            DiscoverySessionArguments::Connect { .. } => DiscoverySessionRole::Alice,
            DiscoverySessionArguments::Accept { .. } => DiscoverySessionRole::Bob,
        }
    }
}

pub enum DiscoverySessionRole {
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

    type Msg = ();

    type Arguments = (
        ActorNamespace,
        DiscoverySessionId,
        NodeId,
        S,
        ActorRef<ToDiscoveryManager<T>>,
        DiscoverySessionArguments<T>,
    );

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (actor_namespace, session_id, remote_node_id, store, manager_ref, args) = args;
        let role = args.role();

        let (tx, rx, walker_ref) = match args {
            DiscoverySessionArguments::Connect { walker_ref } => {
                // Try to establish a direct connection with this node.
                let connection =
                    connect::<T>(remote_node_id, DISCOVERY_PROTOCOL_ID, actor_namespace).await?;
                let (tx, rx) = connection.open_bi().await?;
                (tx, rx, Some(walker_ref))
            }
            DiscoverySessionArguments::Accept { connection } => {
                let (tx, rx) = connection.accept_bi().await?;
                (tx, rx, None)
            }
        };

        // Establish bi-directional QUIC stream as part of the direct connection and use CBOR
        // encoding for message framing.
        let mut tx = into_cbor_sink::<NaiveDiscoveryMessage<T, NodeId, NodeInfo>, _>(tx);
        let mut rx = into_cbor_stream::<NaiveDiscoveryMessage<T, NodeId, NodeInfo>, _>(rx);

        // Run the discovery protocol.
        let protocol = NaiveDiscoveryProtocol::<S, _, T, NodeId, NodeInfo>::new(
            store,
            SubscriptionInfo::<T>::new(),
            remote_node_id,
        );
        let result = match role {
            DiscoverySessionRole::Alice => protocol.alice(&mut tx, &mut rx).await?,
            DiscoverySessionRole::Bob => protocol.bob(&mut tx, &mut rx).await?,
        };

        // Inform walker about result of this discovery session if we've initiated it.
        if let Some(walker_ref) = walker_ref {
            let _ = walker_ref.send_message(ToDiscoveryWalker::NextNode(Some(result.clone())));
        }

        // Inform manager about results as well.
        let _ = manager_ref.send_message(ToDiscoveryManager::FinishSession(session_id, result));

        // Stop this actor for good.
        myself.stop(None);

        Ok(())
    }
}
