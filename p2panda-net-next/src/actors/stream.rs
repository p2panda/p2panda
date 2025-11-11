// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stream actor.
//!
//! This actor forms the coordination layer between the external API and the sync and gossip
//! sub-systems. It is not responsible for spawning or respawning actors, that role is carried out
//! by the stream supervisor actor.
use std::collections::HashMap;

/// Stream actor name.
pub const STREAM: &str = "net.stream";

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, call, cast};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;

use crate::TopicId;
use crate::actors::ActorNamespace;
use crate::actors::gossip::ToGossip;
use crate::network::{FromNetwork, ToNetwork};
use crate::topic_streams::{EphemeralStream, EphemeralStreamSubscription};

pub enum ToStream {
    /// Create a stream for the topic ID and return a publishing handle.
    CreateEphemeralStream(TopicId, RpcReplyPort<EphemeralStream>),

    /// Return a subscription handle for the given topic ID.
    EphemeralSubscription(TopicId, RpcReplyPort<Option<EphemeralStreamSubscription>>),

    /// Unsubscribe from an ephemeral stream for the given topic ID.
    UnsubscribeEphemeral(TopicId),
}

pub struct StreamState {
    actor_namespace: ActorNamespace,
    gossip_actor: ActorRef<ToGossip>,
    sync_actor: ActorRef<()>,
    to_gossip_senders: HashMap<TopicId, Sender<ToNetwork>>,
    from_gossip_senders: HashMap<TopicId, BroadcastSender<FromNetwork>>,
}

#[derive(Default)]
pub struct Stream;

impl ThreadLocalActor for Stream {
    type State = StreamState;
    type Msg = ToStream;
    type Arguments = (ActorNamespace, ActorRef<()>, ActorRef<ToGossip>);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (actor_namespace, sync_actor, gossip_actor) = args;

        let to_gossip_senders = HashMap::new();
        let from_gossip_senders = HashMap::new();

        let state = StreamState {
            actor_namespace,
            gossip_actor,
            sync_actor,
            to_gossip_senders,
            from_gossip_senders,
        };

        Ok(state)
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToStream::CreateEphemeralStream(topic_id, reply) => {
                // TODO: Ask address book for all peers interested in this topic id.
                let peers = Vec::new();

                // Check if we're already subscribed.
                let stream = if let Some(to_gossip_tx) = state.to_gossip_senders.get(&topic_id) {
                    // Inform the gossip actor about the latest set of peers for this topic id.
                    cast!(state.gossip_actor, ToGossip::JoinPeers(topic_id, peers))?;

                    EphemeralStream::new(
                        topic_id,
                        to_gossip_tx.clone(),
                        state.actor_namespace.clone(),
                    )
                } else {
                    // Register a new session with the gossip actor.
                    let (to_gossip_tx, from_gossip_tx) =
                        call!(state.gossip_actor, ToGossip::Subscribe, topic_id, peers)?;

                    // Store the gossip sender which is used to publish messages into the
                    // topic.
                    state
                        .to_gossip_senders
                        .insert(topic_id, to_gossip_tx.clone());

                    // Store the gossip sender. This can be used to create a broadcast receiver
                    // when the user calls `.subscribe()` on `EphemeralStream`.
                    state.from_gossip_senders.insert(topic_id, from_gossip_tx);

                    EphemeralStream::new(topic_id, to_gossip_tx, state.actor_namespace.clone())
                };

                // Ignore any potential send error; it's not a concern of this actor.
                let _ = reply.send(stream);
            }
            ToStream::EphemeralSubscription(topic_id, reply) => {
                if let Some(from_gossip_tx) = state.from_gossip_senders.get(&topic_id) {
                    let from_gossip_rx = from_gossip_tx.subscribe();

                    let subscription = EphemeralStreamSubscription::new(topic_id, from_gossip_rx);

                    let _ = reply.send(Some(subscription));
                } else {
                    let _ = reply.send(None);
                }
            }
            ToStream::UnsubscribeEphemeral(topic_id) => {
                // Drop all senders associated with the topic id..
                let _ = state.to_gossip_senders.remove(&topic_id);
                let _ = state.from_gossip_senders.remove(&topic_id);

                // Tell the gossip actor to unsubscribe from this topic id.
                cast!(state.gossip_actor, ToGossip::Unsubscribe(topic_id))?;
            }
        }

        Ok(())
    }
}
