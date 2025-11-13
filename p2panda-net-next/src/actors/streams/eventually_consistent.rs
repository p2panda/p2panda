// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eventually consistent streams actor.
//!
//! This actor forms the coordination layer between the external API and the sync and gossip
//! sub-systems. It is not responsible for spawning or respawning actors, that role is carried out
//! by the stream supervisor actor.
use std::collections::HashMap;

/// Eventually consistent streams actor name.
pub const EVENTUALLY_CONSISTENT_STREAMS: &str = "net.streams.eventually_consistent";

use ractor::concurrency::broadcast;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort, call, cast};
use tokio::sync::broadcast::{self, Receiver as BroadcastReceiver, Sender as BroadcastSender};
use tokio::sync::mpsc::Sender;

use crate::TopicId;
use crate::actors::gossip::ToGossip;
use crate::actors::sync::{SYNC_MANAGER, SyncManager, ToSyncManager};
use crate::actors::{ActorNamespace, with_namespace};
use crate::network::{FromNetwork, ToNetwork};
use crate::streams::eventually_consistent::{
    EventuallyConsistentStream, EventuallyConsistentSubscription,
};

type IsLiveModeEnabled = bool;

pub enum ToEventuallyConsistentStreams {
    /// Create an eventually consistent stream for the topic ID and return a publishing handle.
    Create(
        TopicId,
        IsLiveModeEnabled,
        RpcReplyPort<EventuallyConsistentStream>,
    ),

    /// Return an eventually consistent subscription handle for the given topic ID.
    Subscribe(
        TopicId,
        RpcReplyPort<Option<EventuallyConsistentSubscription>>,
    ),

    /// Unsubscribe from an eventually consistent stream for the given topic ID.
    Unsubscribe(TopicId),
}

/// Mapping of topic ID to the sender channels of the associated gossip overlay.
type GossipSenders = HashMap<TopicId, (Sender<ToNetwork>, BroadcastSender<FromNetwork>)>;

/// Mapping of topic ID to the receiver channel from the associated sync mananger.
// TODO: Receiver message type may be incorrect (`FromSync` maybe).
type SyncReceivers = HashMap<TopicId, BroadcastReceiver<FromNetwork>>;

type SyncManagers = HashMap<TopicId, ActorRef<ToSyncManager>>;

pub struct EventuallyConsistentStreamsState {
    actor_namespace: ActorNamespace,
    gossip_actor: ActorRef<ToGossip>,
    gossip_senders: GossipSenders,
    sync_managers: SyncManagers,
    sync_receivers: SyncReceivers,
    stream_thread_pool: ThreadLocalActorSpawner,
}

#[derive(Default)]
pub struct EventuallyConsistentStreams;

impl ThreadLocalActor for EventuallyConsistentStreams {
    type State = EventuallyConsistentStreamsState;
    type Msg = ToEventuallyConsistentStreams;
    type Arguments = (ActorNamespace, ActorRef<ToGossip>);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (actor_namespace, gossip_actor) = args;

        let gossip_senders = HashMap::new();
        let sync_receivers = HashMap::new();
        let sync_managers = HashMap::new();

        // Sync manager actors are all spawned in a dedicated thread.
        let stream_thread_pool = ThreadLocalActorSpawner::new();

        let state = EventuallyConsistentStreamsState {
            actor_namespace,
            gossip_actor,
            gossip_senders,
            sync_managers,
            sync_receivers,
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
            ToEventuallyConsistentStreams::Create(topic_id, live_mode, reply) => {
                // TODO: Ask address book for all peers interested in this topic id.
                let peers = Vec::new();

                // Check if we're already subscribed.
                let stream = if let Some(sync_manager_actor) = state.sync_managers.get(&topic_id) {
                    // Inform the gossip actor about the latest set of peers for this topic id.
                    if let Some((to_gossip_tx, _)) = state.gossip_senders.get(&topic_id) {
                        cast!(state.gossip_actor, ToGossip::JoinPeers(topic_id, peers))?;
                    }

                    EventuallyConsistentStream::new(
                        state.actor_namespace.clone(),
                        topic_id,
                        sync_manager_actor.clone(),
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

                    // This is used to send sync messages to the associated eventually consistent
                    // stream handle(s). We use a broadcast channel to allow multiple handles to
                    // the same topic id (with all receiving each message).
                    let (from_sync_tx, from_sync_rx) = broadcast::channel(256);

                    // Store the sync receiver so it can later be used to create an
                    // `EventuallyConsistentSubscription` (if required).
                    state.sync_receivers.insert(topic_id, from_sync_rx);

                    // TODO: Pass the from_sync_tx sender into the sync manager actor.
                    //
                    // Spawn a sync manager for this topic_id.
                    let (sync_manager_actor, _) = SyncManager::spawn_linked(
                        Some(with_namespace(SYNC_MANAGER, &state.actor_namespace)),
                        (),
                        myself.clone().into(),
                        state.stream_thread_pool.clone(),
                    )
                    .await?;

                    state
                        .sync_managers
                        .insert(topic_id, sync_manager_actor.clone());

                    EventuallyConsistentStream::new(
                        state.actor_namespace.clone(),
                        topic_id,
                        sync_manager_actor,
                    )
                };

                // Ignore any potential send error; it's not a concern of this actor.
                let _ = reply.send(stream);
            }
            ToEventuallyConsistentStreams::Subscribe(topic_id, reply) => {
                if let Some(from_sync_rx) = state.sync_receivers.get(&topic_id) {
                    let subscription =
                        EventuallyConsistentSubscription::new(topic_id, from_sync_rx.resubscribe());

                    let _ = reply.send(Some(subscription));
                } else {
                    let _ = reply.send(None);
                }
            }
            ToEventuallyConsistentStreams::Unsubscribe(topic_id) => {
                // Tell the gossip actor to unsubscribe from this topic id.
                cast!(state.gossip_actor, ToGossip::Unsubscribe(topic_id))?;

                // Drop all senders and receivers associated with the topic id.
                state.gossip_senders.remove(&topic_id);
                state.sync_receivers.remove(&topic_id);
            }
        }

        Ok(())
    }
}

// TODO: Supervision for failed and terminated sync manager instances.
