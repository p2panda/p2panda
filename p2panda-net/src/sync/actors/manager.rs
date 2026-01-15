// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync actor.
//!
//! This actor forms the coordination layer between the external API and the sync and gossip
//! sub-systems.
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use iroh::endpoint::Connection;
use iroh::protocol::ProtocolHandler;
use p2panda_sync::FromSync;
use p2panda_sync::topic_handshake::{
    TopicHandshakeAcceptor, TopicHandshakeEvent, TopicHandshakeMessage,
};
use p2panda_sync::traits::{Protocol, SyncManager as SyncManagerTrait};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorId, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use tokio::sync::{RwLock, broadcast};
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::gossip::{Gossip, GossipEvent, GossipHandle};
use crate::iroh_endpoint::{Endpoint, to_public_key};
use crate::sync::actors::{ToTopicManager, TopicManager};
use crate::utils::ShortFormat;
use crate::{NodeId, ProtocolId, TopicId};

type IsLiveModeEnabled = bool;

/// Constant to mix a given topic with to derive a new one.
///
/// This value was generated randomly to guarantee no collisions.
const GOSSIP_TOPIC_MIX_VALUE: TopicId = [
    253, 6, 251, 217, 173, 228, 215, 244, 130, 181, 150, 142, 220, 244, 49, 219, 35, 94, 163, 197,
    229, 93, 143, 227, 97, 61, 38, 202, 63, 250, 26, 233,
];

pub enum ToSyncManager<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    /// Create stream for this topic and return related manager.
    Create(
        TopicId,
        IsLiveModeEnabled,
        RpcReplyPort<ActorRef<ToTopicManager<M::Message>>>,
    ),

    /// Subscribe to the given topic to receive incoming sync events.
    Subscribe(
        TopicId,
        RpcReplyPort<Option<broadcast::Receiver<FromSync<M::Event>>>>,
    ),

    /// Close all streams for the given topic.
    Close(TopicId),

    /// Initiate sync session.
    InitiateSync(TopicId, NodeId),

    /// Accept sync session.
    Accept(NodeId, TopicId, Connection),

    /// End sync session.
    EndSync(TopicId, NodeId),

    /// Register iroh connection handler.
    RegisterProtocol,
}

/// Mapping of topic to the receiver channel from the associated sync manager.
type TopicManagerReceivers<E> = HashMap<TopicId, broadcast::Receiver<FromSync<E>>>;

/// Mapping of the topic to the regarding manager.
struct TopicManagers<T> {
    topic_manager_map: HashMap<TopicId, (ActorRef<ToTopicManager<T>>, IsLiveModeEnabled)>,
    actor_topic_map: HashMap<ActorId, TopicId>,
}

/// Mapping of topic to the regarding gossip overlays dealing with the membership handling.
type GossipHandles = HashMap<TopicId, (GossipHandle, JoinHandle<()>)>;

/// Mapping between the "mixed" topic (key) used for gossip and it's "original" version (value)
/// used by sync.
type GossipTopicMap = Arc<RwLock<HashMap<TopicId, TopicId>>>;

impl<T> Default for TopicManagers<T> {
    fn default() -> Self {
        Self {
            topic_manager_map: Default::default(),
            actor_topic_map: Default::default(),
        }
    }
}

pub struct SyncManagerState<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    protocol_id: ProtocolId,
    endpoint: Endpoint,
    gossip: Gossip,
    gossip_handles: GossipHandles,
    topic_managers: TopicManagers<M::Message>,
    sync_receivers: TopicManagerReceivers<M::Event>,
    gossip_topics: GossipTopicMap,
    sync_config: M::Config,
    thread_pool: ThreadLocalActorSpawner,
}

impl<M> SyncManagerState<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    /// Drop all internal state associated with the given topic.
    fn drop_topic_state(&mut self, topic: &TopicId) {
        self.topic_managers.topic_manager_map.remove(topic);
        self.sync_receivers.remove(topic);

        // Dropping the gossip handle will unsubscribe us from the gossip topic and remove it from
        // the address book.
        if let Some((_, handle)) = self.gossip_handles.remove(topic) {
            // Close task running HyParView membership logic.
            handle.abort();
        }
    }

    /// Join gossip overlay to use HyParView membership algorithm for peer sampling.
    ///
    /// The spawned task listens for "neighbour up" events of that overlay and informs the manager
    /// to initiate sync sessions with the new node in the "active view".
    async fn spawn_membership_task(
        &mut self,
        myself: &ActorRef<ToSyncManager<M>>,
        topic: TopicId,
    ) -> Result<(), ActorProcessingErr> {
        // To avoid collisions when topics are re-used across the application for different
        // purposes (membership algorithms aiding sync protocols or ephemeral messaging gossip
        // overlays), we're defining a constant with which topics from the user will be mixed to
        // derive a new one.
        let gossip_topic = derive_topic(topic, GOSSIP_TOPIC_MIX_VALUE);
        self.gossip_topics.write().await.insert(gossip_topic, topic);

        debug!(
            sync_topic = topic.fmt_short(),
            gossip_topic = gossip_topic.fmt_short(),
            "join gossip overlay for peer-sampling",
        );

        // Join gossip overlay to use HyParView membership algorithm for peer sampling.
        //
        // This will subscribe us to the gossip topic and add it to the address book.
        let gossip_handle = self.gossip.stream(gossip_topic).await?;

        // Listen for events of HyParView who entered or left the "active view". This informs with
        // whom we're running sync sessions with.
        let gossip_events_handle = {
            let mut events = self.gossip.events().await?;
            let myself = myself.clone();
            let gossip_topics = self.gossip_topics.clone();

            tokio::spawn(async move {
                loop {
                    let Ok(event) = events.recv().await else {
                        // Events stream seized, close task.
                        break;
                    };

                    let (topic_from_event, nodes, is_initiate) = match event {
                        GossipEvent::Joined { topic, ref nodes } => {
                            (topic, Vec::from_iter(nodes.iter().cloned()), true)
                        }
                        GossipEvent::NeighbourUp { node, topic } => (topic, vec![node], true),
                        GossipEvent::NeighbourDown { node, topic } => (topic, vec![node], false),
                        GossipEvent::Left { .. } => {
                            continue;
                        }
                    };

                    let Some(sync_topic) =
                        gossip_topics.read().await.get(&topic_from_event).cloned()
                    else {
                        continue;
                    };

                    for node in nodes {
                        let message = if is_initiate {
                            ToSyncManager::InitiateSync(sync_topic, node)
                        } else {
                            ToSyncManager::EndSync(sync_topic, node)
                        };

                        if myself.send_message(message).is_err() {
                            // Actor stopped, close task.
                            break;
                        }
                    }
                }
            })
        };

        self.gossip_handles
            .insert(topic, (gossip_handle, gossip_events_handle));

        Ok(())
    }
}

pub struct SyncManager<M> {
    _phantom: PhantomData<M>,
}

impl<M> Default for SyncManager<M> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<M> ThreadLocalActor for SyncManager<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    type State = SyncManagerState<M>;

    type Msg = ToSyncManager<M>;

    type Arguments = (ProtocolId, M::Config, Endpoint, Gossip);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (protocol_id, sync_config, endpoint, gossip) = args;

        let gossip_handles = HashMap::new();
        let sync_receivers = HashMap::new();
        let sync_managers = Default::default();

        // Sync manager actors are all spawned in a dedicated thread.
        let thread_pool = ThreadLocalActorSpawner::new();

        // Automatically register protocol handler on start.
        let _ = myself.cast(ToSyncManager::RegisterProtocol);

        Ok(SyncManagerState {
            protocol_id,
            endpoint,
            gossip,
            gossip_handles,
            topic_managers: sync_managers,
            gossip_topics: Arc::default(),
            sync_receivers,
            sync_config,
            thread_pool,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Close all active sync sessions.
        for (_, (actor, _)) in state.topic_managers.topic_manager_map.drain() {
            actor.send_message(ToTopicManager::CloseAll)?;
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
            ToSyncManager::RegisterProtocol => {
                debug!(
                    protocol_id = state.protocol_id.fmt_short(),
                    "register sync protocol",
                );

                state
                    .endpoint
                    .accept(
                        state.protocol_id.clone(),
                        SyncProtocolHandler {
                            stream_ref: myself.clone(),
                        },
                    )
                    .await?;
            }
            ToSyncManager::Create(topic, live_mode, reply) => {
                // Check if we're already subscribed.
                if let Some((sync_manager_ref, _)) =
                    state.topic_managers.topic_manager_map.get(&topic)
                {
                    let _ = reply.send(sync_manager_ref.clone());
                    return Ok(());
                }

                debug!(topic = topic.fmt_short(), live_mode, "create sync manager");

                // Join gossip overlay to use HyParView membership algorithm for peer sampling.
                state.spawn_membership_task(&myself, topic).await?;

                // This is used to send sync messages to the associated stream handle(s). We use a
                // broadcast channel to allow multiple handles to the same topic.
                let (from_sync_tx, from_sync_rx) = broadcast::channel(256);

                // Store the sync receiver so it can later be used to create a subscription
                // instance by the user.
                state.sync_receivers.insert(topic, from_sync_rx);

                // TODO: Pass the from_sync_tx sender into the sync manager actor.
                //
                // Spawn a sync manager for this topic.
                let (sync_manager_ref, _) = TopicManager::<M>::spawn_linked(
                    None,
                    (
                        state.protocol_id.clone(),
                        topic,
                        state.sync_config.clone(),
                        from_sync_tx,
                        state.endpoint.clone(),
                    ),
                    myself.clone().into(),
                    state.thread_pool.clone(),
                )
                .await?;

                state
                    .topic_managers
                    .topic_manager_map
                    .insert(topic, (sync_manager_ref.clone(), live_mode));

                state
                    .topic_managers
                    .actor_topic_map
                    .insert(sync_manager_ref.get_id(), topic);

                let _ = reply.send(sync_manager_ref);
            }
            ToSyncManager::Subscribe(topic, reply) => {
                if let Some(from_sync_rx) = state.sync_receivers.get(&topic) {
                    let subscription = from_sync_rx.resubscribe();
                    let _ = reply.send(Some(subscription));
                } else {
                    let _ = reply.send(None);
                }
            }
            ToSyncManager::Close(topic) => {
                // Close all sync sessions running over this topic.
                if let Some((actor, _)) = state.topic_managers.topic_manager_map.get(&topic) {
                    actor.send_message(ToTopicManager::CloseAll)?;
                }

                // Drop the sync manager state for this topic.
                if let Some((sync_manager, _)) =
                    state.topic_managers.topic_manager_map.remove(&topic)
                {
                    state
                        .topic_managers
                        .actor_topic_map
                        .remove(&sync_manager.get_id());

                    // Finish processing all messages in the manager's queue and then kill it.
                    sync_manager.drain()?;
                }

                // Drop all channels and handles associated with the topic. The removed gossip
                // overlay will automatically remove the entry from the address book.
                state.drop_topic_state(&topic);

                debug!(topic = topic.fmt_short(), "close sync manager");
            }
            ToSyncManager::InitiateSync(topic, node_id) => {
                if let Some((sync_manager_actor, live_mode)) =
                    state.topic_managers.topic_manager_map.get(&topic)
                {
                    debug!(
                        topic = topic.fmt_short(),
                        node_id = node_id.fmt_short(),
                        "initiate sync session",
                    );

                    sync_manager_actor.send_message(ToTopicManager::Initiate {
                        node_id,
                        topic,
                        live_mode: *live_mode,
                    })?;
                }
            }
            ToSyncManager::Accept(node_id, topic, connection) => {
                if let Some((sync_manager_actor, live_mode)) =
                    state.topic_managers.topic_manager_map.get(&topic)
                {
                    debug!(
                        topic = topic.fmt_short(),
                        node_id = node_id.fmt_short(),
                        "accept sync session",
                    );

                    sync_manager_actor.send_message(ToTopicManager::Accept {
                        node_id,
                        topic,
                        live_mode: *live_mode,
                        connection,
                    })?;
                }
            }
            ToSyncManager::EndSync(topic, node_id) => {
                if let Some((sync_manager_actor, _)) =
                    state.topic_managers.topic_manager_map.get(&topic)
                {
                    debug!(
                        topic = topic.fmt_short(),
                        node_id = node_id.fmt_short(),
                        "end sync session",
                    );

                    sync_manager_actor.send_message(ToTopicManager::Close { node_id })?;
                }
            }
        }

        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        myself: ActorRef<Self::Msg>,
        message: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SupervisionEvent::ActorStarted(actor) => {
                let actor_id = actor.get_id();
                if let Some(topic) = state.topic_managers.actor_topic_map.get(&actor_id) {
                    debug!(
                        %actor_id,
                        topic = %topic.fmt_short(),
                        "received ready from sync manager"
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if let Some(topic) = state.topic_managers.actor_topic_map.remove(&actor_id) {
                    debug!(
                        %actor_id,
                        topic = %topic.fmt_short(),
                        "sync manager terminated: {reason:?}",
                    );

                    // Drop all state associated with the terminated sync manager.
                    state.drop_topic_state(&topic);
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                // We do not respawn the sync manager if it fails. Instead, we simply drop all
                // state. This means that the user will receive an error if they try to interact
                // with a handle for the associated stream.
                let actor_id = actor.get_id();
                if let Some(topic) = state.topic_managers.actor_topic_map.remove(&actor_id) {
                    warn!(
                        %actor_id,
                        topic = %topic.fmt_short(),
                        "sync manager failed: {panic_msg:#?}",
                    );

                    myself.send_message(ToSyncManager::Close(topic))?;
                }
            }
            _ => (),
        }

        Ok(())
    }
}

struct SyncProtocolHandler<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    stream_ref: ActorRef<ToSyncManager<M>>,
}

impl<M> std::fmt::Debug for SyncProtocolHandler<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyncProtocolHandler").finish()
    }
}

impl<M> ProtocolHandler for SyncProtocolHandler<M>
where
    M: SyncManagerTrait<TopicId> + Send + 'static,
{
    async fn accept(
        &self,
        connection: iroh::endpoint::Connection,
    ) -> Result<(), iroh::protocol::AcceptError> {
        let node_id = to_public_key(connection.remote_id());
        let (tx, rx) = connection.accept_bi().await?;

        // As we are accepting a sync session here we don't yet know the topic which the initiator
        // choses themselves. This "topic handshake" step takes place here before accepting the
        // actual sync session. We may choose to reject the sync session if the handshake resolves
        // to a topic we aren't subscribed to.

        // Establish bi-directional QUIC stream as part of the direct connection and use CBOR
        // encoding for message framing.
        let mut tx = into_cbor_sink::<TopicHandshakeMessage<TopicId>, _>(tx);
        let mut rx = into_cbor_stream::<TopicHandshakeMessage<TopicId>, _>(rx);

        // Channels for sending and receiving protocol events.
        //
        // We don't need to observe these events here as the topic is returned as output when the
        // protocol completes, so these channels exist only to satisfy the API.
        let (event_tx, _event_rx) =
            futures_channel::mpsc::channel::<TopicHandshakeEvent<TopicId>>(128);
        let protocol = TopicHandshakeAcceptor::new(event_tx);
        let topic = protocol
            .run(&mut tx, &mut rx)
            .await
            .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;

        // We know the topic now and send an accept message to the stream actor where it will then
        // be routed to the correct sync manager.
        self.stream_ref
            .send_message(ToSyncManager::Accept(node_id, topic, connection))
            .map_err(|err| iroh::protocol::AcceptError::from_err(err))?;

        Ok(())
    }
}

/// Hash the concatenation of a topic with a given value to derive new topic.
fn derive_topic(topic: TopicId, value: impl AsRef<[u8]>) -> TopicId {
    p2panda_core::Hash::new([topic.as_ref(), value.as_ref()].concat()).into()
}
