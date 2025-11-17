// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ephemeral streams actor.
//!
//! This actor forms a coordination layer between the external API and the gossip sub-systems.
//!
//! No actors are spawned or respawned by the ephemeral streams actor, that role is carried out
//! by the stream supervisor.
use std::collections::HashMap;

/// Ephemeral streams actor name.
pub const EPHEMERAL_STREAMS: &str = "net.streams.ephemeral";

use p2panda_discovery::address_book::NodeInfo;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, call, cast, registry};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::gossip::ToGossip;
use crate::actors::sync::SyncManager;
use crate::actors::{ActorNamespace, with_namespace};
use crate::network::{FromNetwork, ToNetwork};
use crate::streams::ephemeral::{EphemeralStream, EphemeralSubscription};

pub enum ToEphemeralStreams {
    /// Create an ephemeral stream for the topic ID and return a publishing handle.
    Create(TopicId, RpcReplyPort<EphemeralStream>),

    /// Return an ephemeral subscription handle for the given topic ID.
    Subscribe(TopicId, RpcReplyPort<Option<EphemeralSubscription>>),

    /// Close all ephemeral streams for the given topic ID.
    Close(TopicId),

    /// Return `true` if there are any active ephemeral streams for the given topic ID.
    IsActive(TopicId, RpcReplyPort<bool>),
}

/// Mapping of topic ID to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (Sender<ToNetwork>, BroadcastSender<FromNetwork>)>;

pub struct EphemeralStreamsState {
    actor_namespace: ActorNamespace,
    gossip_actor: ActorRef<ToGossip>,
    gossip_senders: GossipSenders,
}

impl EphemeralStreamsState {
    /// Internal helper to get a reference to the address book actor.
    fn address_book_actor(&self) -> Option<ActorRef<ToAddressBook<()>>> {
        if let Some(address_book_actor) =
            registry::where_is(with_namespace(ADDRESS_BOOK, &self.actor_namespace))
        {
            let actor: ActorRef<ToAddressBook<()>> = address_book_actor.into();

            Some(actor)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct EphemeralStreams;

impl ThreadLocalActor for EphemeralStreams {
    type State = EphemeralStreamsState;
    type Msg = ToEphemeralStreams;
    type Arguments = (ActorNamespace, ActorRef<ToGossip>);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (actor_namespace, gossip_actor) = args;

        let gossip_senders = HashMap::new();

        let state = EphemeralStreamsState {
            actor_namespace,
            gossip_actor,
            gossip_senders,
        };

        Ok(state)
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToEphemeralStreams::Create(topic_id, reply) => {
                let address_book_actor = state
                    .address_book_actor()
                    .expect("address book actor should be available");

                // Retrieve all known nodes for the given topic id.
                let node_infos = call!(
                    address_book_actor,
                    ToAddressBook::NodeInfosByTopicIds,
                    vec![topic_id]
                )
                .expect("address book actor should handle call");

                // We are only interested in the id for each node.
                let node_ids = node_infos.iter().map(|node_info| node_info.id()).collect();

                // Check if we're already subscribed.
                let stream = if let Some((to_gossip_tx, _)) = state.gossip_senders.get(&topic_id) {
                    // Inform the gossip actor about the latest set of nodes for this topic id.
                    cast!(state.gossip_actor, ToGossip::JoinPeers(topic_id, node_ids))?;

                    EphemeralStream::new(
                        topic_id,
                        to_gossip_tx.clone(),
                        state.actor_namespace.clone(),
                    )
                } else {
                    // Register a new session with the gossip actor.
                    let (to_gossip_tx, from_gossip_tx) =
                        call!(state.gossip_actor, ToGossip::Subscribe, topic_id, node_ids)?;

                    // Store the gossip senders.
                    //
                    // `from_gossip_tx` is used to create a broadcast receiver when the user calls
                    // `subscribe()` on `EphemeralStream`.
                    state
                        .gossip_senders
                        .insert(topic_id, (to_gossip_tx.clone(), from_gossip_tx));

                    EphemeralStream::new(topic_id, to_gossip_tx, state.actor_namespace.clone())
                };

                // Ignore any potential send error; it's not a concern of this actor.
                let _ = reply.send(stream);
            }
            ToEphemeralStreams::Subscribe(topic_id, reply) => {
                if let Some((_, from_gossip_tx)) = state.gossip_senders.get(&topic_id) {
                    let from_gossip_rx = from_gossip_tx.subscribe();

                    let subscription = EphemeralSubscription::new(topic_id, from_gossip_rx);

                    let _ = reply.send(Some(subscription));
                } else {
                    let _ = reply.send(None);
                }
            }
            ToEphemeralStreams::Close(topic_id) => {
                // Tell the gossip actor to unsubscribe from this topic id.
                cast!(state.gossip_actor, ToGossip::Unsubscribe(topic_id))?;

                // Drop all senders associated with the topic id.
                state.gossip_senders.remove(&topic_id);
            }
            ToEphemeralStreams::IsActive(topic_id, reply) => {
                let is_active = state.gossip_senders.contains_key(&topic_id);
                let _ = reply.send(is_active);
            }
        }

        Ok(())
    }
}
