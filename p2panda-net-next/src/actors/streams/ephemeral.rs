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

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, call, cast};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;

use crate::TopicId;
use crate::actors::ActorNamespace;
use crate::actors::gossip::ToGossip;
use crate::actors::sync::SyncManager;
use crate::network::{FromNetwork, ToNetwork};
use crate::streams::ephemeral::{EphemeralStream, EphemeralSubscription};

pub enum ToEphemeralStreams {
    /// Create an ephemeral stream for the topic ID and return a publishing handle.
    Create(TopicId, RpcReplyPort<EphemeralStream>),

    /// Return an ephemeral subscription handle for the given topic ID.
    Subscribe(TopicId, RpcReplyPort<Option<EphemeralSubscription>>),

    /// Unsubscribe from an ephemeral stream for the given topic ID.
    Unsubscribe(TopicId),
}

/// Mapping of topic ID to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (Sender<ToNetwork>, BroadcastSender<FromNetwork>)>;

pub struct EphemeralStreamsState {
    actor_namespace: ActorNamespace,
    gossip_actor: ActorRef<ToGossip>,
    gossip_senders: GossipSenders,
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
                // TODO: Ask address book for all peers interested in this topic id.
                let peers = Vec::new();

                // Check if we're already subscribed.
                let stream = if let Some((to_gossip_tx, _)) = state.gossip_senders.get(&topic_id) {
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
            ToEphemeralStreams::Unsubscribe(topic_id) => {
                // Tell the gossip actor to unsubscribe from this topic id.
                cast!(state.gossip_actor, ToGossip::Unsubscribe(topic_id))?;

                // Drop all senders associated with the topic id.
                state.gossip_senders.remove(&topic_id);
            }
        }

        Ok(())
    }
}
