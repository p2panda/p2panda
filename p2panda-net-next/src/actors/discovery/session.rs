// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::marker::PhantomData;

use p2panda_discovery::address_book::AddressBookStore;
use ractor::thread_local::ThreadLocalActor;
use ractor::{Actor, ActorProcessingErr, ActorRef};

use crate::actors::discovery::walker::ToDiscoveryWalker;
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

impl<S, T> ThreadLocalActor for DiscoverySession<S, T>
where
    S: AddressBookStore<T, NodeId, NodeInfo> + Clone + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
    T: Send + 'static,
{
    type State = DiscoverySessionState;

    type Msg = ();

    type Arguments = (
        NodeId,
        ActorRef<ToDiscoveryWalker<T>>,
        ApplicationArguments,
        S,
    );

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (node_id, walker_ref, args, store) = args;

        Ok(DiscoverySessionState {})
    }
}
