// SPDX-License-Identifier: MIT OR Apache-2.0

//! Stream actor.
//!
//! This actor forms the coordination layer between the external API and the sync and gossip
//! sub-systems. It is not responsible for spawning or respawning actors, that role is carried out
//! by the stream supervisor actor.
use std::collections::HashMap;

// TODO(glyph): Consider renaming this to avoid conflict with `topic_streams::Stream` type.
/// Stream actor name.
pub const STREAM: &str = "net.stream";

use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{call, cast, ActorProcessingErr, ActorRef, RpcReplyPort};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;

use crate::actors::gossip::ToGossip;
use crate::actors::sync::SyncManager;
use crate::actors::ActorNamespace;
use crate::network::{FromNetwork, ToNetwork};
use crate::topic_streams::{
    EphemeralStream, EphemeralStreamSubscription, Stream as TopicStream, StreamSubscription,
};
use crate::TopicId;

pub enum ToStream {
    /// Create an ephemeral stream for the topic ID and return a publishing handle.
    CreateEphemeralStream(TopicId, RpcReplyPort<EphemeralStream>),

    /// Return an ephemeral subscription handle for the given topic ID.
    EphemeralSubscription(TopicId, RpcReplyPort<Option<EphemeralStreamSubscription>>),

    /// Unsubscribe from an ephemeral stream for the given topic ID.
    UnsubscribeEphemeral(TopicId),

    /// Create an eventually-consistent stream for the topic ID and return a publishing handle.
    CreateStream(TopicId, RpcReplyPort<TopicStream>),

    /// Return an eventually-consistent subscription handle for the given topic ID.
    Subscription(TopicId, RpcReplyPort<Option<StreamSubscription>>),

    /// Unsubscribe from an eventually-consistent stream for the given topic ID.
    Unsubscribe(TopicId),
}

/// Mapping of topic ID to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (Sender<ToNetwork>, BroadcastSender<FromNetwork>)>;

pub struct StreamState {
    actor_namespace: ActorNamespace,
    gossip_actor: ActorRef<ToGossip>,
    sync_actor: ActorRef<()>,
    gossip_senders: GossipSenders,
    stream_thread_pool: ThreadLocalActorSpawner,
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

        let gossip_senders = HashMap::new();

        // Sync manager actors are all spawned in a dedicated thread.
        let stream_thread_pool = ThreadLocalActorSpawner::new();

        let state = StreamState {
            actor_namespace,
            gossip_actor,
            sync_actor,
            gossip_senders,
            stream_thread_pool,
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
            ToStream::CreateEphemeralStream(topic_id, reply) => {
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
                    // `from_gossip_tx` can be used to create a broadcast receiver when the user
                    // calls `subscribe()` on `EphemeralStream`.
                    state
                        .gossip_senders
                        .insert(topic_id, (to_gossip_tx.clone(), from_gossip_tx));

                    EphemeralStream::new(topic_id, to_gossip_tx, state.actor_namespace.clone())
                };

                // Ignore any potential send error; it's not a concern of this actor.
                let _ = reply.send(stream);
            }
            ToStream::EphemeralSubscription(topic_id, reply) => {
                if let Some((_, from_gossip_tx)) = state.gossip_senders.get(&topic_id) {
                    let from_gossip_rx = from_gossip_tx.subscribe();

                    let subscription = EphemeralStreamSubscription::new(topic_id, from_gossip_rx);

                    let _ = reply.send(Some(subscription));
                } else {
                    let _ = reply.send(None);
                }
            }
            ToStream::UnsubscribeEphemeral(topic_id) => {
                // Drop all senders associated with the topic id..
                let _ = state.gossip_senders.remove(&topic_id);

                // Tell the gossip actor to unsubscribe from this topic id.
                cast!(state.gossip_actor, ToGossip::Unsubscribe(topic_id))?;
            }
            ToStream::CreateStream(topic_id, reply) => {
                // TODO: Ask address book for all peers interested in this topic id.
                let peers = Vec::new();

                // TODO: Check if we already have a stream for this topic id.
                // If so, clone the required channels and return a "new" stream object.

                // Register a new session with the gossip actor.
                let (to_gossip_tx, from_gossip_tx) =
                    call!(state.gossip_actor, ToGossip::Subscribe, topic_id, peers)?;

                // Spawn a sync manager for this topic_id.
                let (sync_manager_actor, _) = SyncManager::spawn_linked(
                    None,
                    // TODO: Pass in proper args (including the topic id).
                    (),
                    myself.clone().into(),
                    state.stream_thread_pool.clone(),
                )
                .await?;

                todo!()
            }
            ToStream::Subscription(topic_id, reply) => {
                todo!()
            }
            ToStream::Unsubscribe(topic_id) => {
                todo!()
            }
        }

        Ok(())
    }
}
