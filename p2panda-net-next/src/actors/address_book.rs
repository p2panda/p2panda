// SPDX-License-Identifier: MIT OR Apache-2.0

// @TODO(adz): Not sure currently if we need an address book "actor" or if it just a store being
// passed around to actors instead, the latter seems currently closer to what this is (reading &
// writing & persisting values).
use std::collections::{HashMap, HashSet};

use ractor::{Actor, ActorProcessingErr, ActorRef, Message, RpcReplyPort};

use crate::{NodeId, NodeInfo, TopicId};

// @TODO: Remove once used.
#[allow(dead_code)]
pub enum ToAddressBook {
    /// Add a peer address.
    AddAddress(NodeInfo),

    /// Associate a peer with a topic of interest.
    AddTopicId(NodeId, TopicId),

    /// Return all known addresses for the given peer.
    PeerAddresses(NodeId, RpcReplyPort<Vec<NodeInfo>>),

    /// Return all known peer addresses.
    AllPeerAddresses(RpcReplyPort<Vec<NodeInfo>>),

    /// Return a random set of known peers with an interest in the given topic.
    RandomAddressSet(TopicId, usize, RpcReplyPort<Vec<NodeId>>),
}

impl Message for ToAddressBook {}

#[allow(dead_code)]
#[derive(Default)]
pub struct AddressBookState {
    node_addresses: HashMap<NodeId, NodeInfo>,
    node_topic_ids: HashMap<NodeId, HashSet<TopicId>>,
}

#[allow(dead_code)]
pub struct AddressBook;

impl Actor for AddressBook {
    type State = AddressBookState;
    type Msg = ToAddressBook;

    // @TODO: For now we leave out the concept of a `NetworkId` but we may want some way to slice
    // address subsets in the future.
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = AddressBookState::default();
        Ok(state)
    }
}
