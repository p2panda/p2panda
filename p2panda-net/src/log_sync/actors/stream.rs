// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync actor.
//!
//! This actor forms the coordination layer between the external API and the sync and gossip
//! sub-systems.
use std::collections::HashMap;
use std::fmt::Debug;
use std::marker::PhantomData;

use iroh::endpoint::Connection;
use iroh::protocol::ProtocolHandler;
use p2panda_sync::FromSync;
use p2panda_sync::topic_handshake::{
    TopicHandshakeAcceptor, TopicHandshakeEvent, TopicHandshakeMessage,
};
use p2panda_sync::traits::{Protocol, SyncManager as SyncManagerTrait};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorId, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::address_book::AddressBook;
use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::gossip::{Gossip, GossipEvent, GossipHandle};
use crate::iroh_endpoint::{Endpoint, to_public_key};
use crate::log_sync::actors::{SYNC_PROTOCOL_ID, ToTopicManager, TopicManager};
use crate::utils::ShortFormat;
use crate::{NodeId, TopicId};

type IsLiveModeEnabled = bool;

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

type GossipHandles = HashMap<TopicId, (GossipHandle, JoinHandle<()>)>;

/// Mapping of topic to the receiver channel from the associated sync manager.
type TopicManagerReceivers<E> = HashMap<TopicId, broadcast::Receiver<FromSync<E>>>;

struct TopicManagers<T> {
    topic_manager_map: HashMap<TopicId, (ActorRef<ToTopicManager<T>>, IsLiveModeEnabled)>,
    actor_topic_map: HashMap<ActorId, TopicId>,
}

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
    address_book: AddressBook,
    endpoint: Endpoint,
    gossip: Gossip,
    gossip_handles: GossipHandles,
    topic_managers: TopicManagers<M::Message>,
    sync_receivers: TopicManagerReceivers<M::Event>,
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

        if let Some((_, handle)) = self.gossip_handles.remove(topic) {
            // Close task running HyParView membership logic.
            handle.abort();
        }
    }

    /// Inform address book about our current topics by updating our own entry.
    async fn update_address_book(&self) -> Result<(), ActorProcessingErr> {
        // TODO
        self.address_book
            .set_ephemeral_messaging_topics(
                self.endpoint.node_id(),
                self.gossip_handles.keys().cloned(),
            )
            .await?;
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
    M: SyncManagerTrait<TopicId> + Debug + Send + 'static,
{
    type State = SyncManagerState<M>;

    type Msg = ToSyncManager<M>;

    type Arguments = (M::Config, AddressBook, Endpoint, Gossip);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (sync_config, address_book, endpoint, gossip) = args;

        let gossip_handles = HashMap::new();
        let sync_receivers = HashMap::new();
        let sync_managers = Default::default();

        // Sync manager actors are all spawned in a dedicated thread.
        let thread_pool = ThreadLocalActorSpawner::new();

        // Automatically register protocol handler on start.
        let _ = myself.cast(ToSyncManager::RegisterProtocol);

        Ok(SyncManagerState {
            address_book,
            endpoint,
            gossip,
            gossip_handles,
            topic_managers: sync_managers,
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
        for (topic, (actor, _)) in state.topic_managers.topic_manager_map.drain() {
            actor.send_message(ToTopicManager::CloseAll { topic })?;
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
                state
                    .endpoint
                    .accept(
                        SYNC_PROTOCOL_ID,
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

                // Join gossip overlay to use HyParView membership algorithm for peer sampling.
                let gossip_handle = state.gossip.stream(topic).await?;

                // Listen for "neighbour up" events of HyParView which informs with whom we're
                // initiating sync sessions with.
                let gossip_events_handle = {
                    let mut events = state.gossip.events().await?;
                    let myself = myself.clone();

                    tokio::spawn(async move {
                        loop {
                            let Ok(event) = events.recv().await else {
                                // Events stream seized, close task.
                                break;
                            };

                            let GossipEvent::NeighbourUp {
                                node,
                                topic: event_topic,
                            } = event
                            else {
                                continue;
                            };

                            if topic != event_topic {
                                continue;
                            }

                            if myself
                                .send_message(ToSyncManager::InitiateSync(event_topic, node))
                                .is_err()
                            {
                                // Actor stopped, close task.
                                break;
                            }
                        }
                    })
                };

                state
                    .gossip_handles
                    .insert(topic, (gossip_handle, gossip_events_handle));

                // This is used to send sync messages to the associated stream handle(s). We
                // use a broadcast channel to allow multiple handles to the same topic.
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

                // Inform address book about the newly subscribed topic.
                state.update_address_book().await?;

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
                    actor.send_message(ToTopicManager::CloseAll { topic })?;
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

                // Drop all channels and handles associated with the topic.
                state.drop_topic_state(&topic);

                // Inform address book about removed topic.
                state.update_address_book().await?;
            }
            ToSyncManager::InitiateSync(topic, node_id) => {
                if let Some((sync_manager_actor, live_mode)) =
                    state.topic_managers.topic_manager_map.get(&topic)
                {
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
                    sync_manager_actor.send_message(ToTopicManager::Close { node_id, topic })?;
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
                        "sync manager terminated with reason: {reason:?}",
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
                        "sync manager failed with reason: {panic_msg:#?}",
                    );

                    // Drop all state associated with the terminated sync manager.
                    state.drop_topic_state(&topic);
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[derive(Debug)]
struct SyncProtocolHandler<M>
where
    M: SyncManagerTrait<TopicId> + Debug + Send + 'static,
{
    stream_ref: ActorRef<ToSyncManager<M>>,
}

impl<M> ProtocolHandler for SyncProtocolHandler<M>
where
    M: SyncManagerTrait<TopicId> + Debug + Send + 'static,
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
