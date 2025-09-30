// SPDX-License-Identifier: MIT OR Apache-2.0

//! Address book actor for storing and querying peer addresses and topics of interest.
use std::collections::{HashMap, HashSet};

use p2panda_core::PublicKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, RpcReplyPort};
use rand::seq::IteratorRandom;

use crate::TopicId;
use crate::addrs::NodeAddress;

// TODO: Proper configuration.
//
// Add a `persist` flag; if enabled, the address book is loaded from file on actor startup
// and persisted to file on actor shutdown. If not, no addresses are loaded or saved.
//
// Config should probably include a filepath for loading and saving.

pub enum ToAddressBook {
    /// Add a peer address.
    AddAddress(NodeAddress),

    /// Associate a peer with a topic of interest.
    AddTopicId(PublicKey, TopicId),

    /// Return all known addresses for the given peer.
    PeerAddresses(PublicKey, RpcReplyPort<Vec<NodeAddress>>),

    /// Return all known peer addresses.
    AllPeerAddresses(RpcReplyPort<Vec<NodeAddress>>),

    /// Return a random set of known peers with an interest in the given topic.
    RandomAddressSet(TopicId, usize, RpcReplyPort<Vec<PublicKey>>),
}

impl Message for ToAddressBook {}

pub struct AddressBookState {
    peer_addresses: HashMap<PublicKey, HashSet<NodeAddress>>,
    peer_topic_ids: HashMap<PublicKey, HashSet<TopicId>>,
}

pub struct AddressBook;

impl Actor for AddressBook {
    type State = AddressBookState;
    type Msg = ToAddressBook;
    // TODO: For now we leave out the concept of a `NetworkId` but we may want some way to slice
    // address subsets in the future.
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // TODO: Load the address book from disk.

        let peer_addresses = HashMap::new();
        let peer_topic_ids = HashMap::new();

        let state = AddressBookState {
            peer_addresses,
            peer_topic_ids,
        };

        Ok(state)
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
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToAddressBook::AddAddress(node_addr) => {
                let public_key = node_addr.public_key;

                state
                    .peer_addresses
                    .entry(public_key)
                    .or_default()
                    .insert(node_addr);
            }
            ToAddressBook::AddTopicId(public_key, topic_id) => {
                state
                    .peer_topic_ids
                    .entry(public_key)
                    .or_default()
                    .insert(topic_id);
            }
            ToAddressBook::PeerAddresses(public_key, reply) => {
                let mut node_addrs = Vec::new();

                if let Some(addrs) = state.peer_addresses.get(&public_key) {
                    for addr in addrs {
                        node_addrs.push(addr.to_owned())
                    }
                }

                if !reply.is_closed() {
                    let _ = reply.send(node_addrs);
                }
            }
            ToAddressBook::AllPeerAddresses(reply) => {
                let peers = state.peer_addresses.values().flatten().cloned().collect();

                if !reply.is_closed() {
                    let _ = reply.send(peers);
                }
            }
            ToAddressBook::RandomAddressSet(topic_id, sample_len, reply) => {
                // Find all peers interested in the given topic.
                let interested_peers = state.peer_topic_ids.iter().fold(
                    Vec::new(),
                    |mut acc, (public_key, topic_ids)| {
                        if topic_ids.contains(&topic_id) {
                            acc.push(*public_key);
                        }
                        acc
                    },
                );

                // Choose a random subset of the interested peers.
                let random_set_of_interested_peers = interested_peers
                    .iter()
                    .choose_multiple(&mut rand::rng(), sample_len)
                    .into_iter()
                    .cloned()
                    .collect();

                if !reply.is_closed() {
                    let _ = reply.send(random_set_of_interested_peers);
                }
            }
        }

        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // TODO: Persist the address book to disk.

        Ok(())
    }
}
