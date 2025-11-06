// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::marker::PhantomData;

use p2panda_discovery::address_book::AddressBookStore;
use ractor::thread_local::ThreadLocalActor;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::actors::discovery::DISCOVERY_PROTOCOL_ID;
use crate::actors::discovery::walker::ToDiscoveryWalker;
use crate::actors::iroh::connect;
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;

pub struct DiscoverySessionState {}

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

pub enum DiscoverySessionArguments<S, T> {
    Connect {
        node_id: NodeId,
        walker_ref: ActorRef<ToDiscoveryWalker<T>>,
        args: ApplicationArguments,
        store: S,
    },
    Accept {
        node_id: NodeId,
        connection: iroh::endpoint::Connection,
        args: ApplicationArguments,
        store: S,
    },
}

impl<S, T> ThreadLocalActor for DiscoverySession<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
    T: Send + 'static,
{
    type State = DiscoverySessionState;

    type Msg = ();

    type Arguments = DiscoverySessionArguments<S, T>;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        match args {
            DiscoverySessionArguments::Connect {
                node_id,
                walker_ref,
                args,
                store,
            } => {
                // Try to establish a direct connection with this node.
                let connection = connect::<T>(node_id, DISCOVERY_PROTOCOL_ID).await?;
                let (tx, rx) = connection.open_bi().await?;
            }
            DiscoverySessionArguments::Accept {
                node_id,
                connection,
                args,
                store,
            } => {
                let (tx, rx) = connection.open_bi().await?;
            }
        }

        Ok(DiscoverySessionState {})
    }
}
