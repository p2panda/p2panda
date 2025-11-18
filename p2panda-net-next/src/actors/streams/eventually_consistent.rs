// SPDX-License-Identifier: MIT OR Apache-2.0

//! Eventually consistent streams actor.
//!
//! This actor forms the coordination layer between the external API and the sync and gossip
//! sub-systems. It is not responsible for spawning or respawning actors, that role is carried out
//! by the stream supervisor actor.
use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_core::PublicKey;
use p2panda_discovery::address_book::NodeInfo;
use p2panda_sync::SyncManagerEvent;
use p2panda_sync::traits::{Protocol, SyncManager as SyncManagerTrait};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{
    ActorId, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, call, cast, registry,
};
use tokio::sync::broadcast::{self, Receiver as BroadcastReceiver, Sender as BroadcastSender};
use tokio::sync::mpsc::Sender;
use tracing::{debug, warn};

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook};
use crate::actors::gossip::ToGossip;
use crate::actors::streams::ephemeral::{EPHEMERAL_STREAMS, ToEphemeralStreams};
use crate::actors::sync::{SyncManager, ToSyncManager};
use crate::actors::{ActorNamespace, generate_actor_namespace, with_namespace};
use crate::args::ApplicationArguments;
use crate::streams::eventually_consistent::{
    EventuallyConsistentStream, EventuallyConsistentSubscription,
};

/// Eventually consistent streams actor name.
pub const EVENTUALLY_CONSISTENT_STREAMS: &str = "net.streams.eventually_consistent";

type IsLiveModeEnabled = bool;

pub enum ToEventuallyConsistentStreams<E> {
    /// Create an eventually consistent stream for the topic and return a publishing handle.
    Create(
        TopicId,
        IsLiveModeEnabled,
        RpcReplyPort<EventuallyConsistentStream<E>>,
    ),

    /// Return an eventually consistent subscription handle for the given topic.
    Subscribe(
        TopicId,
        RpcReplyPort<Option<EventuallyConsistentSubscription<E>>>,
    ),

    /// Close all eventually consistent streams for the given topic.
    Close(TopicId),

    /// Initiate a sync session.
    InitiateSync(TopicId, PublicKey),

    /// End a sync session.
    EndSync(TopicId, PublicKey),
}

/// Mapping of topic to the sender channels of the associated gossip overlay.
type GossipSenders = HashMap<TopicId, (Sender<Vec<u8>>, BroadcastSender<Vec<u8>>)>;

/// Mapping of topic to the receiver channel from the associated sync manager.
type SyncReceivers<E> = HashMap<TopicId, BroadcastReceiver<SyncManagerEvent<E>>>;

#[derive(Default)]
struct SyncManagers {
    topic_manager_map: HashMap<TopicId, (ActorRef<ToSyncManager>, IsLiveModeEnabled)>,
    actor_topic_map: HashMap<ActorId, TopicId>,
}

pub struct EventuallyConsistentStreamsState<C, E> {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    gossip_actor: ActorRef<ToGossip>,
    gossip_senders: GossipSenders,
    sync_managers: SyncManagers,
    sync_receivers: SyncReceivers<E>,
    sync_config: C,
    stream_thread_pool: ThreadLocalActorSpawner,
}

impl<M, E> EventuallyConsistentStreamsState<M, E> {
    /// Drop all internal state associated with the given topic.
    fn drop_topic_state(&mut self, topic: &TopicId) {
        self.sync_managers.topic_manager_map.remove(topic);
        self.gossip_senders.remove(topic);
        self.sync_receivers.remove(topic);
    }

    /// Unsubscribe from this gossip topic if there aren't any active ephemeral streams for the
    /// given topic.
    async fn unsubscribe_from_gossip(&mut self, topic: TopicId) -> Result<(), ActorProcessingErr> {
        if let Some(ephemeral_streams_actor) =
            registry::where_is(with_namespace(EPHEMERAL_STREAMS, &self.actor_namespace))
        {
            let actor: ActorRef<ToEphemeralStreams> = ephemeral_streams_actor.into();

            // Ask the ephemeral streams actor if there are any active streams for this topic.
            let active_ephemeral_stream = call!(actor, ToEphemeralStreams::IsActive, topic)?;

            // If there aren't any active streams, tell the gossip actor to unsubscribe.
            if !active_ephemeral_stream {
                cast!(self.gossip_actor, ToGossip::Unsubscribe(topic))?;
            }
        }

        Ok(())
    }

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

    /// Inform address book about our current topics by updating our own entry.
    fn update_address_book(&self) {
        if let Some(address_book_ref) = self.address_book_actor()
            && let Err(err) =
                address_book_ref.send_message(ToAddressBook::SetEphemeralMessagingTopics(
                    self.args.public_key,
                    HashSet::from_iter(self.sync_receivers.keys().cloned()),
                ))
        {
            warn!("failed updating local topics in address book: {err:#?}")
        }
    }
}

pub struct EventuallyConsistentStreams<M> {
    _phantom: PhantomData<M>,
}

impl<M> Default for EventuallyConsistentStreams<M> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<M> ThreadLocalActor for EventuallyConsistentStreams<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
    M::Error: StdError + Send + Sync + 'static,
    M::Protocol: Send + 'static,
    <M::Protocol as Protocol>::Event: Clone + Debug + Send + Sync + 'static,
    <M::Protocol as Protocol>::Error: StdError + Send + Sync + 'static,
{
    type State = EventuallyConsistentStreamsState<M::Config, <M::Protocol as Protocol>::Event>;

    type Msg = ToEventuallyConsistentStreams<<M::Protocol as Protocol>::Event>;

    type Arguments = (ApplicationArguments, ActorRef<ToGossip>, M::Config);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, gossip_actor, sync_config) = args;
        let actor_namespace = generate_actor_namespace(&args.public_key);

        let gossip_senders = HashMap::new();
        let sync_receivers = HashMap::new();
        let sync_managers = Default::default();

        // Sync manager actors are all spawned in a dedicated thread.
        let stream_thread_pool = ThreadLocalActorSpawner::new();

        Ok(EventuallyConsistentStreamsState {
            actor_namespace,
            args,
            gossip_actor,
            gossip_senders,
            sync_managers,
            sync_receivers,
            sync_config,
            stream_thread_pool,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Close all active sync sessions.
        for (topic, (actor, _)) in state.sync_managers.topic_manager_map.drain() {
            actor.send_message(ToSyncManager::CloseAll { topic })?;
        }

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToEventuallyConsistentStreams::Create(topic, live_mode, reply) => {
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
                let stream = if let Some((sync_manager_actor, _)) =
                    state.sync_managers.topic_manager_map.get(&topic)
                {
                    // Inform the gossip actor about the latest set of nodes for this topic.
                    if state.gossip_senders.contains_key(&topic) {
                        cast!(state.gossip_actor, ToGossip::JoinPeers(topic, node_ids))?;
                    }

                    EventuallyConsistentStream::new(
                        state.actor_namespace.clone(),
                        topic,
                        sync_manager_actor.clone(),
                    )
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
                        .insert(topic, (to_gossip_tx.clone(), from_gossip_tx));

                    // This is used to send sync messages to the associated eventually consistent
                    // stream handle(s). We use a broadcast channel to allow multiple handles to
                    // the same topic (with all receiving each message).
                    let (from_sync_tx, from_sync_rx) = broadcast::channel(256);

                    // Store the sync receiver so it can later be used to create an
                    // `EventuallyConsistentSubscription` (if required).
                    state.sync_receivers.insert(topic, from_sync_rx);

                    // TODO: Pass the from_sync_tx sender into the sync manager actor.
                    //
                    // Spawn a sync manager for this topic.
                    let (sync_manager_actor, _) = SyncManager::<M>::spawn_linked(
                        // TODO: Consider naming each actor (they will need a unique ID).
                        None,
                        (
                            state.actor_namespace.clone(),
                            topic,
                            state.sync_config.clone(),
                            from_sync_tx,
                        ),
                        myself.clone().into(),
                        state.stream_thread_pool.clone(),
                    )
                    .await?;

                    state
                        .sync_managers
                        .topic_manager_map
                        .insert(topic, (sync_manager_actor.clone(), live_mode));

                    state
                        .sync_managers
                        .actor_topic_map
                        .insert(sync_manager_actor.get_id(), topic);

                    EventuallyConsistentStream::new(
                        state.actor_namespace.clone(),
                        topic,
                        sync_manager_actor,
                    )
                };

                // Inform address book about newly added topic.
                state.update_address_book();

                // Ignore any potential send error; it's not a concern of this actor.
                let _ = reply.send(stream);
            }
            ToEventuallyConsistentStreams::Subscribe(topic, reply) => {
                if let Some(from_sync_rx) = state.sync_receivers.get(&topic) {
                    let subscription =
                        EventuallyConsistentSubscription::new(topic, from_sync_rx.resubscribe());

                    let _ = reply.send(Some(subscription));
                } else {
                    let _ = reply.send(None);
                }
            }
            ToEventuallyConsistentStreams::Close(topic) => {
                // Tell the gossip actor to unsubscribe from this topic.
                state.unsubscribe_from_gossip(topic).await?;

                // Drop all senders and receivers associated with the topic.
                state.gossip_senders.remove(&topic);
                state.sync_receivers.remove(&topic);

                // Inform address book about removed topic.
                state.update_address_book();

                // Drop the sync manager state for this topic.
                if let Some((sync_manager, _)) =
                    state.sync_managers.topic_manager_map.remove(&topic)
                {
                    state
                        .sync_managers
                        .actor_topic_map
                        .remove(&sync_manager.get_id());

                    // Finish processing all messages in the manager's queue and then kill it.
                    sync_manager.drain()?;
                }
            }
            ToEventuallyConsistentStreams::InitiateSync(topic, node_id) => {
                if let Some((sync_manager_actor, live_mode)) =
                    state.sync_managers.topic_manager_map.get(&topic)
                {
                    sync_manager_actor.send_message(ToSyncManager::Initiate {
                        node_id,
                        topic,
                        live_mode: *live_mode,
                    })?;
                }
            }
            ToEventuallyConsistentStreams::EndSync(topic, node_id) => {
                if let Some((sync_manager_actor, _)) =
                    state.sync_managers.topic_manager_map.get(&topic)
                {
                    sync_manager_actor.send_message(ToSyncManager::Close { node_id, topic })?;
                }
            }
        }

        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(actor) => {
                let actor_id = actor.get_id();
                if let Some(topic) = state.sync_managers.actor_topic_map.get(&actor_id) {
                    debug!(
                        "eventually consistent streams actor: received ready from sync manager #{} for topic {:?}",
                        actor_id, topic
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if let Some(topic) = state.sync_managers.actor_topic_map.remove(&actor_id) {
                    debug!(
                        "eventually consistent streams actor: sync manager #{} over topic {:?} terminated with reason: {:?}",
                        actor_id, topic, reason
                    );

                    // Drop all state associated with the terminated sync manager.
                    state.drop_topic_state(&topic);
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                // NOTE: We do not respawn the sync manager if it fails. Instead, we simply drop
                // all state. This means that the user will receive an error if they try to
                // interact with a handle for the associated stream.

                let actor_id = actor.get_id();
                if let Some(topic) = state.sync_managers.actor_topic_map.remove(&actor_id) {
                    warn!(
                        "eventually consistent streams actor: sync manager #{} over topic {:?} failed with reason: {:?}",
                        actor_id, topic, panic_msg
                    );

                    // Tell the gossip actor to unsubscribe from this topic.
                    state.unsubscribe_from_gossip(topic).await?;

                    // Drop all state associated with the terminated sync manager.
                    state.drop_topic_state(&topic);
                }
            }
            _ => (),
        }

        Ok(())
    }
}
