// SPDX-License-Identifier: MIT OR Apache-2.0

//! An `iroh`-specific gossip actor for message broadcast.
mod healer;
mod joiner;
mod listener;
mod receiver;
mod sender;
mod session;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;

use iroh::Endpoint as IrohEndpoint;
use iroh::EndpointId;
use iroh_gossip::net::Gossip as IrohGossip;
use iroh_gossip::proto::{Config as IrohGossipConfig, DeliveryScope as IrohDeliveryScope};
use p2panda_core::PublicKey;
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorId, ActorProcessingErr, ActorRef, RpcReplyPort, SupervisionEvent, registry};
use tokio::sync::broadcast::{self, Sender as BroadcastSender};
use tokio::sync::mpsc::{self, Sender};
use tokio::sync::oneshot::{self, Sender as OneshotSender};
use tracing::{debug, warn};

use crate::TopicId;
use crate::actors::ActorNamespace;
use crate::actors::gossip::session::{GossipSession, ToGossipSession};
use crate::actors::iroh::register_protocol;
use crate::actors::streams::eventually_consistent::{
    EVENTUALLY_CONSISTENT_STREAMS, ToEventuallyConsistentStreams,
};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::args::ApplicationArguments;
use crate::protocols::hash_protocol_id_with_network_id;
use crate::utils::{from_public_key, to_public_key};

/// Gossip actor name.
pub const GOSSIP: &str = "net.gossip";

pub enum ToGossip {
    /// Accept incoming "gossip protocol" connection requests.
    RegisterProtocol,

    /// Return a handle to the iroh gossip actor.
    ///
    /// This is required when registering the gossip ALPN with the router.
    #[allow(unused)]
    Handle(RpcReplyPort<IrohGossip>),

    /// Subscribe to the given topic, using the given peers as gossip bootstrap nodes.
    ///
    /// Two senders are returned: 1) a sender _into_ the gossip overlay, 2) a sender _out of_ the
    /// gossip overlay. The reason we return the second sender is because it's a broadcast channel
    /// and we need the sender in order to produce receivers by calling `.subscribe()`.
    Subscribe(
        TopicId,
        Vec<PublicKey>,
        #[allow(clippy::type_complexity)] RpcReplyPort<(Sender<Vec<u8>>, BroadcastSender<Vec<u8>>)>,
    ),

    /// Unsubscribe from the given topic.
    Unsubscribe(TopicId),

    /// Join a set of peers on the given gossip topic.
    ///
    /// This event requires a prior subscription to the topic via the `ToGossip::Subscribe`.
    JoinPeers(TopicId, Vec<PublicKey>),

    /// Joined a topic by connecting to the given peers.
    Joined {
        topic: TopicId,
        peers: Vec<PublicKey>,
        session_id: ActorId,
    },

    /// Gained a new, direct neighbor in the gossip overlay.
    NeighborUp {
        node_id: PublicKey,
        session_id: ActorId,
    },

    /// Lost a direct neighbor in the gossip overlay.
    NeighborDown {
        node_id: PublicKey,
        session_id: ActorId,
    },

    /// Received a message from the gossip overlay.
    ReceivedMessage {
        bytes: Vec<u8>,
        #[allow(unused)]
        delivered_from: PublicKey,
        delivery_scope: IrohDeliveryScope,
        topic: TopicId,
        #[allow(unused)]
        session_id: ActorId,
    },

    /// Returns current actor's state for testing purposes.
    #[cfg(test)]
    DebugState(RpcReplyPort<tests::DebugState>),
}

/// Mapping of topic to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (Sender<Vec<u8>>, BroadcastSender<Vec<u8>>)>;

/// Actor references and channels for gossip sessions.
#[derive(Default)]
struct Sessions {
    sessions_by_actor_id: HashMap<ActorId, TopicId>,
    sessions_by_topic: HashMap<TopicId, ActorRef<ToGossipSession>>,
    gossip_senders: GossipSenders,
    gossip_joined_senders: HashMap<ActorId, OneshotSender<u8>>,
}

pub struct GossipState {
    gossip: IrohGossip,
    sessions: Sessions,
    neighbours: HashMap<TopicId, HashSet<PublicKey>>,
    topic_delivery_scopes: HashMap<TopicId, Vec<IrohDeliveryScope>>,
    gossip_thread_pool: ThreadLocalActorSpawner,
    actor_namespace: ActorNamespace,
}

impl GossipState {
    fn drop_topic_state(&mut self, actor_id: &ActorId, topic: &TopicId) {
        self.sessions.sessions_by_topic.remove(topic);
        self.sessions.gossip_senders.remove(topic);
        self.sessions.gossip_joined_senders.remove(actor_id);
        self.neighbours.remove(topic);
    }
}

pub struct Gossip<E> {
    _phantom: PhantomData<E>,
}

impl<E> Default for Gossip<E> {
    fn default() -> Self {
        Self {
            _phantom: Default::default(),
        }
    }
}

impl<E> ThreadLocalActor for Gossip<E>
where
    E: Clone + Debug + Send + Sync + 'static,
{
    type State = GossipState;
    type Msg = ToGossip;
    type Arguments = (ApplicationArguments, IrohEndpoint);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, endpoint) = args;
        let config = IrohGossipConfig::default();

        let actor_namespace = generate_actor_namespace(&to_public_key(endpoint.id()));
        let mixed_alpn = hash_protocol_id_with_network_id(iroh_gossip::ALPN, &args.network_id);
        let gossip = IrohGossip::builder()
            .alpn(mixed_alpn)
            .max_message_size(config.max_message_size)
            .membership_config(config.membership)
            .broadcast_config(config.broadcast)
            .spawn(endpoint);

        let sessions = Sessions::default();
        let neighbours = HashMap::new();
        let topic_delivery_scopes = HashMap::new();

        // Gossip "worker" actors are all spawned in a dedicated thread.
        let gossip_thread_pool = ThreadLocalActorSpawner::new();

        Ok(GossipState {
            gossip,
            sessions,
            neighbours,
            topic_delivery_scopes,
            gossip_thread_pool,
            actor_namespace,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Leave all subscribed topics, send `Disconnect` messages to peers and drop all state and
        // connections.
        state.gossip.shutdown().await?;

        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToGossip::RegisterProtocol => {
                register_protocol(
                    iroh_gossip::ALPN,
                    state.gossip.clone(),
                    state.actor_namespace.clone(),
                )?;
                Ok(())
            }
            ToGossip::Handle(reply) => {
                let gossip = state.gossip.clone();
                let _ = reply.send(gossip);

                Ok(())
            }
            ToGossip::Subscribe(topic, peers, reply) => {
                // Channel to receive messages from the user (to the gossip overlay).
                let (to_gossip_tx, to_gossip_rx) = mpsc::channel(128);

                // Channel to receive messages from the gossip overlay (to the user).
                //
                // NOTE: We ignore `from_gossip_rx` because it will be created in the
                // subscription actor as required by calling `.subscribe()` on the sender.
                let (from_gossip_tx, _from_gossip_rx) = broadcast::channel(128);

                // Oneshot channel to notify the session sender(s) that the overlay has been
                // joined.
                let (gossip_joined_tx, gossip_joined_rx) = oneshot::channel();

                // Convert p2panda public keys to iroh endpoint ids.
                let peers = peers
                    .iter()
                    .map(|key: &PublicKey| from_public_key(*key))
                    .collect();

                // Subscribe to the gossip topic (without waiting for a connection).
                let subscription = state.gossip.subscribe(topic.into(), peers).await?;

                // Spawn the session actor with the gossip topic subscription.
                let (gossip_session_actor, _) = GossipSession::spawn_linked(
                    None,
                    (
                        state.actor_namespace.clone(),
                        topic,
                        subscription,
                        to_gossip_rx,
                        gossip_joined_rx,
                        myself.clone(),
                        state.gossip_thread_pool.clone(),
                    ),
                    myself.clone().into(),
                    state.gossip_thread_pool.clone(),
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

                Ok(())
            }
            ToGossip::Unsubscribe(topic) => {
                // Stop the session associated with this topic.
                if let Some(actor) = state.sessions.sessions_by_topic.remove(&topic) {
                    let actor_id = actor.get_id();
                    state.sessions.sessions_by_actor_id.remove(&actor_id);
                    state.sessions.gossip_joined_senders.remove(&actor_id);

                    actor.stop(Some("received unsubscribe request".to_string()));
                }

                // Drop all associated state.
                state.sessions.gossip_senders.remove(&topic);
                state.neighbours.remove(&topic);
                state.topic_delivery_scopes.remove(&topic);

                Ok(())
            }
            ToGossip::JoinPeers(topic, peers) => {
                // Convert p2panda public keys to iroh endpoint ids.
                let peers: Vec<EndpointId> = peers
                    .iter()
                    .map(|key: &PublicKey| from_public_key(*key))
                    .collect();

                if let Some(session) = state.sessions.sessions_by_topic.get(&topic) {
                    let _ = session.cast(ToGossipSession::JoinPeers(peers.clone()));
                }

                Ok(())
            }
            ToGossip::ReceivedMessage {
                bytes,
                delivered_from: _,
                delivery_scope,
                topic,
                session_id: _,
            } => {
                // Store the delivery scope of the received message.
                state
                    .topic_delivery_scopes
                    .entry(topic)
                    .or_default()
                    .push(delivery_scope);

                // Write the received bytes to all subscribers for the associated topic.
                if let Some((_, from_gossip_tx)) = state.sessions.gossip_senders.get(&topic) {
                    let _number_of_subscribers = from_gossip_tx.send(bytes)?;
                }

                Ok(())
            }
            ToGossip::Joined {
                topic,
                peers,
                session_id,
            } => {
                debug!("joined topic {:?} with peers: {:?}", topic, peers);

                // Inform the gossip sender actor that the overlay has been joined.
                if let Some(gossip_joined_tx) =
                    state.sessions.gossip_joined_senders.remove(&session_id)
                    && gossip_joined_tx.send(1).is_err()
                {
                    warn!("oneshot gossip joined receiver dropped")
                }

                let peer_set = HashSet::from_iter(peers);

                // Store the neighbours with whom we have joined the topic.
                state.neighbours.insert(topic, peer_set);

                Ok(())
            }
            ToGossip::NeighborUp {
                node_id,
                session_id,
            } => {
                // Insert the node into the set of neighbours.
                if let Some(topic) = state.sessions.sessions_by_actor_id.get(&session_id)
                    && let Some(neighbours) = state.neighbours.get_mut(topic)
                {
                    if let Some(eventually_consistent_streams_actor) = registry::where_is(
                        with_namespace(EVENTUALLY_CONSISTENT_STREAMS, &state.actor_namespace),
                    ) {
                        let actor: ActorRef<ToEventuallyConsistentStreams<E>> =
                            eventually_consistent_streams_actor.into();

                        // Ask the eventually consistent streams actor to initiate a sync session
                        // for this topic.
                        actor.send_message(ToEventuallyConsistentStreams::InitiateSync(
                            *topic, node_id,
                        ))?;
                    }

                    neighbours.insert(node_id);
                }

                Ok(())
            }
            ToGossip::NeighborDown {
                node_id,
                session_id,
            } => {
                // Remove the peer from the set of neighbours.
                if let Some(topic) = state.sessions.sessions_by_actor_id.get(&session_id)
                    && let Some(neighbours) = state.neighbours.get_mut(topic)
                {
                    if let Some(eventually_consistent_streams_actor) = registry::where_is(
                        with_namespace(EVENTUALLY_CONSISTENT_STREAMS, &state.actor_namespace),
                    ) {
                        let actor: ActorRef<ToEventuallyConsistentStreams<E>> =
                            eventually_consistent_streams_actor.into();

                        // Ask the eventually consistent streams actor to end any sync sessions
                        // for this topic.
                        actor.send_message(ToEventuallyConsistentStreams::EndSync(
                            *topic, node_id,
                        ))?;
                    }

                    neighbours.remove(&node_id);
                }

                Ok(())
            }
            #[cfg(test)]
            ToGossip::DebugState(reply) => {
                let _ = reply.send(state.into());
                Ok(())
            }
        }
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
                        "gossip actor: received ready from gossip session actor #{} for topic {:?}",
                        actor_id, topic
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if let Some(topic) = state.sessions.sessions_by_actor_id.remove(&actor_id) {
                    debug!(
                        "gossip actor: gossip session #{} over topic {:?} terminated with reason: {:?}",
                        actor_id, topic, reason
                    );

                    // Drop all state associated with the terminated gossip session.
                    state.drop_topic_state(&actor_id, &topic);
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
                        "gossip_actor: gossip session #{} over topic {:?} failed with reason: {}",
                        actor_id, topic, panic_msg
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
