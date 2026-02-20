// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt::Debug;

use iroh::endpoint::QuicTransportConfig;
use p2panda_discovery::psi_hash::{PsiHashDiscoveryProtocol, PsiHashMessage};
use p2panda_discovery::traits::{self, DiscoveryProtocol as _};
use p2panda_store_next::SqliteStore;
use p2panda_store_next::address_book::AddressBookStore;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};

use crate::addrs::NodeInfo;
use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::discovery::actors::{DISCOVERY_PROTOCOL_ID, ToDiscoveryManager};
use crate::iroh_endpoint::Endpoint;
use crate::{NodeId, TopicId};

pub type DiscoverySessionId = u64;

#[derive(Debug, Default)]
pub struct DiscoverySession;

pub enum ToDiscoverySession {
    Initiate(DiscoverySessionArguments),
}

pub struct DiscoverySessionArguments {
    pub my_node_id: NodeId,
    pub remote_node_id: NodeId,
    pub store: SqliteStore<'static>,
    pub endpoint: Endpoint,
    pub manager_ref: ActorRef<ToDiscoveryManager>,
    pub quic_transport_config: QuicTransportConfig,
    pub args: DiscoverySessionRole,
}

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

impl ThreadLocalActor for DiscoverySession {
    type State = ();

    type Msg = ToDiscoverySession;

    type Arguments = DiscoverySessionArguments;

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
        let DiscoverySessionArguments {
            my_node_id,
            remote_node_id,
            store,
            endpoint,
            manager_ref,
            quic_transport_config,
            args,
        } = args;
        let role = args.role();

        let (connection, tx, rx) = match args {
            DiscoverySessionRole::Connect => {
                // Try to establish a direct connection with this node.
                let connection = endpoint
                    .connect_with_config(
                        remote_node_id,
                        DISCOVERY_PROTOCOL_ID,
                        quic_transport_config,
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
        let mut tx = into_cbor_sink::<PsiHashMessage<NodeId, NodeInfo>, _>(tx);
        let mut rx = into_cbor_stream::<PsiHashMessage<NodeId, NodeInfo>, _>(rx);

        // Run the discovery protocol.
        // TODO: Have a timeout to cancel session if it's running overtime.
        let protocol = PsiHashDiscoveryProtocol::<SqliteStore<'_>, _, NodeId, NodeInfo>::new(
            store.clone(),
            LocalTopicsProvider { store, my_node_id },
            my_node_id,
            remote_node_id,
        );

        let result = match role {
            Role::Alice => {
                let result = protocol.alice(&mut tx, &mut rx).await?;
                connection.closed().await;
                result
            }
            Role::Bob => {
                let result = protocol.bob(&mut tx, &mut rx).await?;
                connection.close(0u32.into(), b"done");
                result
            }
        };

        // Inform manager about our results.
        let _ = manager_ref.send_message(ToDiscoveryManager::OnSuccess(myself.clone(), result));

        // Stop this actor for good.
        myself.stop(None);

        Ok(())
    }
}

#[derive(Debug)]
struct LocalTopicsProvider<S> {
    store: S,
    my_node_id: NodeId,
}

impl<S> traits::LocalTopics for LocalTopicsProvider<S>
where
    S: AddressBookStore<NodeId, NodeInfo>,
{
    type Error = <S as AddressBookStore<NodeId, NodeInfo>>::Error;

    async fn topics(&self) -> Result<HashSet<TopicId>, Self::Error> {
        self.store.node_topics(&self.my_node_id).await
    }
}
