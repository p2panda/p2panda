// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ephemeral streams actor.
//!
//! This actor forms a coordination layer between the external API and the gossip sub-systems.
//!
//! No actors are spawned or respawned by the ephemeral streams actor, that role is carried out
//! by the stream supervisor.
use std::collections::{HashMap, HashSet};

use p2panda_discovery::address_book::NodeInfo;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, call, cast, registry};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;
use tracing::warn;

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::gossip::ToGossip;
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::streams::{EphemeralStream, EphemeralSubscription};
use crate::test_utils::ApplicationArguments;

/// Ephemeral streams actor name.
pub const EPHEMERAL_STREAMS: &str = "net.streams.ephemeral";

pub enum ToEphemeralStreams {
    /// Create an ephemeral stream for the topic and return a publishing handle.
    Create(TopicId, RpcReplyPort<EphemeralStream>),

    /// Return an ephemeral subscription handle for the given topic.
    Subscribe(TopicId, RpcReplyPort<Option<EphemeralSubscription>>),

    /// Close all ephemeral streams for the given topic.
    Close(TopicId),

    /// Return `true` if there are any active ephemeral streams for the given topic.
    IsActive(TopicId, RpcReplyPort<bool>),
}

/// Mapping of topic to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (Sender<Vec<u8>>, BroadcastSender<Vec<u8>>)>;

pub struct EphemeralStreamsState {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    gossip_actor: ActorRef<ToGossip>,
    gossip_senders: GossipSenders,
    active_topics: HashSet<TopicId>,
}

impl EphemeralStreamsState {
    /// Internal helper to get a reference to the address book actor.
    fn address_book_actor(&self) -> Option<ActorRef<ToAddressBook>> {
        if let Some(address_book_actor) =
            registry::where_is(with_namespace(ADDRESS_BOOK, &self.actor_namespace))
        {
            let actor: ActorRef<ToAddressBook> = address_book_actor.into();
            Some(actor)
        } else {
            None
        }
    }

    fn add_topic(&mut self, topic: TopicId) {
        self.active_topics.insert(topic);
        self.update_address_book();
    }

    fn remove_topic(&mut self, topic: &TopicId) {
        self.active_topics.remove(topic);
        self.update_address_book();
    }

    /// Inform address book about our current topics by updating our own entry.
    fn update_address_book(&self) {
        if let Some(address_book_ref) = self.address_book_actor()
            && let Err(err) =
                address_book_ref.send_message(ToAddressBook::SetEphemeralMessagingTopics(
                    self.args.public_key,
                    self.active_topics.clone(),
                ))
        {
            warn!("failed updating local topics in address book: {err:#?}")
        }
    }
}

#[derive(Default)]
pub struct EphemeralStreams;

impl ThreadLocalActor for EphemeralStreams {
    type State = EphemeralStreamsState;
    type Msg = ToEphemeralStreams;
    type Arguments = (ApplicationArguments, ActorRef<ToGossip>);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, gossip_actor) = args;
        let actor_namespace = generate_actor_namespace(&args.public_key);

        Ok(EphemeralStreamsState {
            actor_namespace,
            args,
            gossip_actor,
            gossip_senders: HashMap::new(),
            active_topics: HashSet::new(),
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToEphemeralStreams::Create(topic, reply) => {
                let address_book_actor = state
                    .address_book_actor()
                    .expect("address book actor should be available");

                // Retrieve all known nodes for the given topic.
                let node_infos = call!(
                    address_book_actor,
                    ToAddressBook::NodeInfosByEphemeralMessagingTopics,
                    vec![topic]
                )
                .expect("address book actor should handle call");

                // We are only interested in the id for each node.
                let node_ids = node_infos.iter().map(|node_info| node_info.id()).collect();

                // Check if we're already subscribed.
                let stream = if let Some((to_gossip_tx, from_gossip_tx)) =
                    state.gossip_senders.get(&topic)
                {
                    // Inform the gossip actor about the latest set of nodes for this topic.
                    cast!(state.gossip_actor, ToGossip::JoinPeers(topic, node_ids))?;

                    EphemeralStream::new(topic, to_gossip_tx.clone(), from_gossip_tx.clone())
                } else {
                    // Register a new session with the gossip actor.
                    let (to_gossip_tx, from_gossip_tx) =
                        call!(state.gossip_actor, ToGossip::Subscribe, topic, node_ids)?;

                    // Store the gossip senders.
                    //
                    // `from_gossip_tx` is used to create a broadcast receiver when the user calls
                    // `subscribe()` on `EphemeralStream`.
                    state
                        .gossip_senders
                        .insert(topic, (to_gossip_tx.clone(), from_gossip_tx.clone()));
                    state.add_topic(topic);

                    EphemeralStream::new(topic, to_gossip_tx, from_gossip_tx)
                };

                // Ignore any potential send error; it's not a concern of this actor.
                let _ = reply.send(stream);
            }
            ToEphemeralStreams::Subscribe(topic, reply) => {
                if let Some((_, from_gossip_tx)) = state.gossip_senders.get(&topic) {
                    let from_gossip_rx = from_gossip_tx.subscribe();
                    let subscription = EphemeralSubscription::new(topic, from_gossip_rx);
                    let _ = reply.send(Some(subscription));
                } else {
                    let _ = reply.send(None);
                }
            }
            ToEphemeralStreams::Close(topic) => {
                // Tell the gossip actor to unsubscribe from this topic.
                cast!(state.gossip_actor, ToGossip::Unsubscribe(topic))?;

                // Drop all senders associated with the topic.
                state.gossip_senders.remove(&topic);
                state.remove_topic(&topic);
            }
            ToEphemeralStreams::IsActive(topic, reply) => {
                let is_active = state.gossip_senders.contains_key(&topic);
                let _ = reply.send(is_active);
            }
        }

        Ok(())
    }
}
