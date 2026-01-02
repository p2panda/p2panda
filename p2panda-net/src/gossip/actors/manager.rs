// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use iroh_gossip::net::Gossip as IrohGossip;
use iroh_gossip::proto::DeliveryScope as IrohDeliveryScope;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorId, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, warn};

use crate::address_book::AddressBook;
use crate::gossip::GossipConfig;
use crate::gossip::actors::session::{GossipSession, ToGossipSession};
use crate::gossip::events::GossipEvent;
use crate::hash_protocol_id_with_network_id;
use crate::iroh_endpoint::{Endpoint, from_public_key};
use crate::utils::ShortFormat;
use crate::{NodeId, TopicId};

pub enum ToGossipManager {
    /// Accept incoming "gossip protocol" connection requests.
    RegisterProtocol,

    /// Subscribe to the given topic, using the given nodes as gossip bootstrap nodes.
    ///
    /// Two senders are returned: 1) a sender _into_ the gossip overlay, 2) a sender _out of_ the
    /// gossip overlay. The reason we return the second sender is because it's a broadcast channel
    /// and we need the sender in order to produce receivers by calling `.subscribe()`.
    Subscribe(
        TopicId,
        Vec<NodeId>,
        #[allow(clippy::type_complexity)]
        RpcReplyPort<(mpsc::Sender<Vec<u8>>, broadcast::Sender<Vec<u8>>)>,
    ),

    /// Unsubscribe from the given topic.
    Unsubscribe(TopicId),

    /// Join a set of nodes on the given gossip topic.
    ///
    /// This event requires a prior subscription to the topic via the `ToGossip::Subscribe`.
    JoinNodes(TopicId, Vec<NodeId>),

    /// Joined a topic by connecting to the given nodes.
    Joined {
        topic: TopicId,
        nodes: Vec<NodeId>,
        session_id: ActorId,
    },

    /// Gained a new, direct neighbor in the gossip overlay.
    NeighborUp {
        node_id: NodeId,
        session_id: ActorId,
    },

    /// Lost a direct neighbor in the gossip overlay.
    NeighborDown {
        node_id: NodeId,
        session_id: ActorId,
    },

    /// Received a message from the gossip overlay.
    ReceivedMessage {
        bytes: Vec<u8>,
        #[allow(unused)]
        delivered_from: NodeId,
        delivery_scope: IrohDeliveryScope,
        topic: TopicId,
        #[allow(unused)]
        session_id: ActorId,
    },

    /// Subscribe to system events.
    Events(RpcReplyPort<broadcast::Receiver<GossipEvent>>),
}

/// Mapping of topic to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (mpsc::Sender<Vec<u8>>, broadcast::Sender<Vec<u8>>)>;

/// Actor references and channels for gossip sessions.
#[derive(Default)]
pub struct Sessions {
    pub sessions_by_actor_id: HashMap<ActorId, TopicId>,
    pub sessions_by_topic: HashMap<TopicId, ActorRef<ToGossipSession>>,
    pub gossip_senders: GossipSenders,
    pub gossip_joined_senders: HashMap<ActorId, oneshot::Sender<()>>,
}

pub struct GossipManagerState {
    my_node_id: NodeId,
    address_book: AddressBook,
    endpoint: Endpoint,
    pool: ThreadLocalActorSpawner,
    gossip: Option<IrohGossip>,
    sessions: Sessions,
    neighbours: HashMap<TopicId, HashSet<NodeId>>,
    events_tx: broadcast::Sender<GossipEvent>,
}

impl GossipManagerState {
    fn drop_topic_state(&mut self, actor_id: &ActorId, topic: &TopicId) {
        self.sessions.sessions_by_topic.remove(topic);
        self.sessions.gossip_senders.remove(topic);
        self.sessions.gossip_joined_senders.remove(actor_id);
        self.neighbours.remove(topic);
    }
}

#[derive(Default)]
pub struct GossipManager;

impl ThreadLocalActor for GossipManager {
    type State = GossipManagerState;

    type Msg = ToGossipManager;

    type Arguments = (GossipConfig, AddressBook, Endpoint);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (config, address_book, endpoint) = args;
        let my_node_id = endpoint.node_id();

        let mixed_alpn = hash_protocol_id_with_network_id(iroh_gossip::ALPN, endpoint.network_id());

        let gossip = IrohGossip::builder()
            .alpn(mixed_alpn)
            .max_message_size(config.max_message_size)
            .membership_config(config.membership)
            .broadcast_config(config.broadcast)
            .spawn(endpoint.endpoint().await?);

        let sessions = Sessions::default();
        let neighbours = HashMap::new();
        let (events_tx, _) = broadcast::channel(64);

        // Gossip "worker" actors are all spawned in a dedicated thread.
        let pool = ThreadLocalActorSpawner::new();

        // Automatically register gossip ALPN after actor started.
        myself.send_message(ToGossipManager::RegisterProtocol)?;

        Ok(GossipManagerState {
            my_node_id,
            address_book,
            endpoint,
            pool,
            gossip: Some(gossip),
            sessions,
            neighbours,
            events_tx,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Leave all subscribed topics, send `Disconnect` messages to nodes and drop all state and
        // connections.
        if let Some(gossip) = state.gossip.take() {
            // Make sure the endpoint has all the time it needs to gracefully shut down while other
            // processes might already drop the whole actor.
            tokio::task::spawn(async move {
                if let Err(err) = gossip.shutdown().await {
                    warn!("gossip failed during shutdown: {err:?}");
                }
            });
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
            ToGossipManager::RegisterProtocol => {
                state
                    .endpoint
                    .accept(
                        iroh_gossip::ALPN,
                        state
                            .gossip
                            .as_ref()
                            .expect("gossip was initialised when actor started")
                            .clone(),
                    )
                    .await?;
            }
            ToGossipManager::Subscribe(topic, nodes, reply) => {
                // Channel to receive messages from the user (to the gossip overlay).
                let (to_gossip_tx, to_gossip_rx) = mpsc::channel(128);

                // Channel to receive messages from the gossip overlay (to the user).
                //
                // NOTE: We ignore `from_gossip_rx` because it will be created in the subscription
                // actor as required by calling `.subscribe()` on the sender.
                let (from_gossip_tx, _from_gossip_rx) = broadcast::channel(128);

                // Oneshot channel to notify the session sender(s) that the overlay has been
                // joined.
                let (gossip_joined_tx, gossip_joined_rx) = oneshot::channel();

                // Convert p2panda public keys to iroh endpoint ids.
                let nodes = nodes
                    .iter()
                    .map(|key: &NodeId| from_public_key(*key))
                    .collect();

                // Subscribe to the gossip topic (without waiting for a connection).
                let subscription = state
                    .gossip
                    .as_ref()
                    .expect("gossip was initialised when actor started")
                    .subscribe(topic.into(), nodes)
                    .await?;

                // Spawn the session actor with the gossip topic subscription.
                let (gossip_session_actor, _) = GossipSession::spawn_linked(
                    None,
                    (
                        state.my_node_id,
                        state.address_book.clone(),
                        topic,
                        subscription,
                        to_gossip_rx,
                        gossip_joined_rx,
                        myself.clone(),
                        state.pool.clone(),
                    ),
                    myself.clone().into(),
                    state.pool.clone(),
                )
                .await?;

                // Associate the session actor id with the topic.
                let gossip_session_actor_id = gossip_session_actor.get_id();
                state
                    .sessions
                    .sessions_by_actor_id
                    .insert(gossip_session_actor_id, topic);

                // Associate the topic with the session actor.
                state
                    .sessions
                    .sessions_by_topic
                    .insert(topic, gossip_session_actor);

                // Associate the session actor with the gossip joined sender.
                state
                    .sessions
                    .gossip_joined_senders
                    .insert(gossip_session_actor_id, gossip_joined_tx);

                // Associate the topic with the senders to and from gossip.
                state
                    .sessions
                    .gossip_senders
                    .insert(topic, (to_gossip_tx.clone(), from_gossip_tx.clone()));

                // Return sender / receiver pair to the user.
                let _ = reply.send((to_gossip_tx, from_gossip_tx));
            }
            ToGossipManager::Unsubscribe(topic) => {
                // Stop the session associated with this topic.
                if let Some(actor) = state.sessions.sessions_by_topic.remove(&topic) {
                    let actor_id = actor.get_id();
                    state.sessions.gossip_joined_senders.remove(&actor_id);

                    actor.stop(Some("received unsubscribe request".to_string()));
                }

                // Drop all associated state.
                state.sessions.gossip_senders.remove(&topic);
                state.neighbours.remove(&topic);
            }
            ToGossipManager::JoinNodes(topic, nodes) => {
                // Convert p2panda public keys to iroh endpoint ids.
                let nodes: Vec<iroh::EndpointId> = nodes
                    .iter()
                    .map(|key: &NodeId| from_public_key(*key))
                    .collect();

                if let Some(session) = state.sessions.sessions_by_topic.get(&topic) {
                    let _ = session.cast(ToGossipSession::JoinNodes(nodes.clone()));
                }
            }
            ToGossipManager::ReceivedMessage { bytes, topic, .. } => {
                if let Some((_, from_gossip_tx)) = state.sessions.gossip_senders.get(&topic) {
                    let _number_of_subscribers = from_gossip_tx.send(bytes)?;
                }
            }
            ToGossipManager::Joined {
                topic,
                nodes,
                session_id,
            } => {
                debug!(topic = %topic.fmt_short(), nodes = %nodes.fmt_short(), "joined topic");

                // Inform the gossip sender actor that the overlay has been joined.
                if let Some(gossip_joined_tx) =
                    state.sessions.gossip_joined_senders.remove(&session_id)
                    && gossip_joined_tx.send(()).is_err()
                {
                    warn!("oneshot gossip joined receiver dropped")
                }

                let nodes = HashSet::from_iter(nodes.into_iter());
                state.neighbours.insert(topic, nodes.clone());

                let _ = state.events_tx.send(GossipEvent::Joined { topic, nodes });
            }
            ToGossipManager::NeighborUp {
                node_id,
                session_id,
            } => {
                let Some(topic) = state.sessions.sessions_by_actor_id.get(&session_id) else {
                    return Ok(());
                };

                let Some(neighbors) = state.neighbours.get_mut(topic) else {
                    return Ok(());
                };

                let _ = state.events_tx.send(GossipEvent::NeighbourUp {
                    topic: *topic,
                    node: node_id,
                });

                neighbors.insert(node_id);
            }
            ToGossipManager::NeighborDown {
                node_id,
                session_id,
            } => {
                let Some(topic) = state.sessions.sessions_by_actor_id.get(&session_id) else {
                    return Ok(());
                };

                let Some(neighbors) = state.neighbours.get_mut(topic) else {
                    return Ok(());
                };

                let _ = state.events_tx.send(GossipEvent::NeighbourDown {
                    topic: *topic,
                    node: node_id,
                });

                neighbors.remove(&node_id);
            }
            ToGossipManager::Events(reply) => {
                let _ = reply.send(state.events_tx.subscribe());
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
                if let Some(topic) = state.sessions.sessions_by_actor_id.get(&actor_id) {
                    debug!(
                        %actor_id,
                        topic = topic.fmt_short(),
                        "received ready from gossip session",
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if let Some(topic) = state.sessions.sessions_by_actor_id.remove(&actor_id) {
                    debug!(
                        %actor_id,
                        topic = topic.fmt_short(),
                        "gossip session terminated: {reason:?}",
                    );

                    // Drop all state associated with the terminated gossip session.
                    state.drop_topic_state(&actor_id, &topic);

                    let _ = state.events_tx.send(GossipEvent::Left { topic });
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                // NOTE: We do not respawn the session if it fails. Instead, we simply drop the
                // gossip message sender to the user. The user is expected to handle the error on
                // the receiver and resubscribe to the topic if they wish.
                //
                // TODO: We rather want to handle the resubscribe internally. If the root gossip
                // actor holds a clone of `to_network_rx` and `from_network_tx` then it's possible
                // to spawn a replacement for the failed session (while maintaining the original
                // channels established for message passing with the user). After some threshold
                // number of failures in a given timespan we drop the channels completely and
                // return an error to the user.

                let actor_id = actor.get_id();
                if let Some(topic) = state.sessions.sessions_by_actor_id.remove(&actor_id) {
                    warn!(
                        %actor_id,
                        topic = topic.fmt_short(),
                        "gossip session failed: {panic_msg:#?}",
                    );

                    // Drop all state associated with the failed gossip session.
                    state.drop_topic_state(&actor_id, &topic);
                }
            }
            _ => (),
        }

        Ok(())
    }
}
