// SPDX-License-Identifier: MIT OR Apache-2.0

//! Address book for storing and querying node information and topics of interest.
use std::marker::PhantomData;
use std::time::Duration;

use ractor::{Actor, ActorProcessingErr, ActorRef, Message, RpcReplyPort};

use crate::TopicId;
use crate::addrs::{NodeId, NodeInfo};
use crate::store::AddressBookStore;

pub const ADDRESS_BOOK: &str = "address_book";

pub enum ToAddressBook<T> {
    /// Registers information about a node.
    ///
    /// Outdated node information will automatically be ignored.
    AddNodeInfo(NodeInfo),

    /// Associate a node with set of topics of interest for eventual consistent messaging via sync.
    SetTopics(NodeId, Vec<T>),

    /// Associate a node with a set of topic ids for ephemeral messaging.
    SetTopicIds(NodeId, Vec<TopicId>),

    /// Return info for the given node.
    NodeInfo(NodeId, RpcReplyPort<Option<NodeInfo>>),

    /// Return infos of nodes which are interested in _at least one_ of the topics in the given
    /// topic set.
    NodeInfosByTopics(Vec<T>, RpcReplyPort<Vec<NodeInfo>>),

    NodeInfosByTopicId(TopicId, RpcReplyPort<Option<NodeInfo>>),

    /// Return all known node infos.
    AllNodeInfos(RpcReplyPort<Vec<NodeInfo>>),

    RandomNode(RpcReplyPort<Option<NodeInfo>>),

    RandomBootstrapNode(RpcReplyPort<Option<NodeInfo>>),

    RandomNodeByTopic(T, RpcReplyPort<Option<NodeInfo>>),

    RandomNodeByTopicId(TopicId, RpcReplyPort<Option<NodeInfo>>),

    RemoveNodeInfosOlderThan(Duration, RpcReplyPort<usize>),

    RemoveNodeInfo(NodeId),
}

impl<T> Message for ToAddressBook<T> where T: Send + Sync + 'static {}

pub struct AddressBookState;

pub struct AddressBook<T, S> {
    store: S,
    _marker: PhantomData<T>,
}

impl<T, S> AddressBook<T, S> {
    pub fn new(store: S) -> Self {
        Self {
            store,
            _marker: PhantomData,
        }
    }
}

impl<T, S> Actor for AddressBook<T, S>
where
    S: AddressBookStore + Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    type State = AddressBookState;

    type Msg = ToAddressBook<T>;

    // TODO: For now we leave out the concept of a `NetworkId` but we may want some way to slice
    // address subsets in the future.
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(AddressBookState {})
    }

    async fn post_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        _message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }
}
