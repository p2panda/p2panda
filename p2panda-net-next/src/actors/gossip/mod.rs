// SPDX-License-Identifier: MIT OR Apache-2.0

//! An `iroh`-specific gossip actor for message broadcast.
mod healer;
mod joiner;
mod listener;
mod receiver;
mod sender;
mod session;

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
use crate::actors::streams::eventually_consistent::{
    EVENTUALLY_CONSISTENT_STREAMS, ToEventuallyConsistentStreams,
};
use crate::actors::{generate_actor_namespace, with_namespace};
use crate::network::FromNetwork;
use crate::utils::{from_public_key, to_public_key};

/// Gossip actor name.
pub const GOSSIP: &str = "net.gossip";

pub enum ToGossip {
    /// Return a handle to the iroh gossip actor.
    ///
    /// This is required when registering the gossip ALPN with the router.
    Handle(RpcReplyPort<IrohGossip>),

    /// Subscribe to the given topic, using the given peers as gossip bootstrap nodes.
    Subscribe(
        TopicId,
        Vec<PublicKey>,
        RpcReplyPort<(Sender<Vec<u8>>, BroadcastSender<FromNetwork>)>,
    ),

    /// Unsubscribe from the given topic.
    Unsubscribe(TopicId),

    /// Join a set of peers on the given gossip topic.
    ///
    /// This event requires a prior subscription to the topic via the `ToGossip::Subscribe`.
    JoinPeers(TopicId, Vec<PublicKey>),

    /// Joined a topic by connecting to the given peers.
    Joined {
        topic_id: TopicId,
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
        delivered_from: PublicKey,
        delivery_scope: IrohDeliveryScope,
        topic_id: TopicId,
        session_id: ActorId,
    },

    /// Returns current actor's state for testing purposes.
    #[cfg(test)]
    DebugState(RpcReplyPort<tests::DebugState>),
}

/// Mapping of topic ID to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (Sender<Vec<u8>>, BroadcastSender<FromNetwork>)>;

/// Actor references and channels for gossip sessions.
#[derive(Default)]
struct Sessions {
    sessions_by_actor_id: HashMap<ActorId, TopicId>,
    sessions_by_topic_id: HashMap<TopicId, ActorRef<ToGossipSession>>,
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
    fn drop_topic_state(&mut self, actor_id: &ActorId, topic_id: &TopicId) {
        self.sessions.sessions_by_topic_id.remove(topic_id);
        self.sessions.gossip_senders.remove(topic_id);
        self.sessions.gossip_joined_senders.remove(actor_id);
        self.neighbours.remove(topic_id);
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
    type Arguments = IrohEndpoint;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        endpoint: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let config = IrohGossipConfig::default();

        let actor_namespace = generate_actor_namespace(&to_public_key(endpoint.id()));

        let gossip = IrohGossip::builder()
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
            ToGossip::Handle(reply) => {
                let gossip = state.gossip.clone();
                let _ = reply.send(gossip);

                Ok(())
            }
            ToGossip::Subscribe(topic_id, peers, reply) => {
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
                let subscription = state.gossip.subscribe(topic_id.into(), peers).await?;

                // Spawn the session actor with the gossip topic subscription.
                let (gossip_session_actor, _) = GossipSession::spawn_linked(
                    None,
                    (
                        state.actor_namespace.clone(),
                        topic_id,
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
                    .insert(gossip_session_actor_id, topic_id);

                // Associate the topic id with the session actor.
                state
                    .sessions
                    .sessions_by_topic_id
                    .insert(topic_id, gossip_session_actor);

                // Associate the session actor with the gossip joined sender.
                state
                    .sessions
                    .gossip_joined_senders
                    .insert(gossip_session_actor_id, gossip_joined_tx);

                // Associate the topic id with the senders to and from gossip.
                state
                    .sessions
                    .gossip_senders
                    .insert(topic_id, (to_gossip_tx.clone(), from_gossip_tx.clone()));

                // Return sender / receiver pair to the user.
                let _ = reply.send((to_gossip_tx, from_gossip_tx));

                Ok(())
            }
            ToGossip::Unsubscribe(topic_id) => {
                // Stop the session associated with this topic id.
                if let Some(actor) = state.sessions.sessions_by_topic_id.remove(&topic_id) {
                    let actor_id = actor.get_id();
                    state.sessions.sessions_by_actor_id.remove(&actor_id);
                    state.sessions.gossip_joined_senders.remove(&actor_id);

                    actor.stop(Some("received unsubscribe request".to_string()));
                }

                // Drop all associated state.
                state.sessions.gossip_senders.remove(&topic_id);
                state.neighbours.remove(&topic_id);
                state.topic_delivery_scopes.remove(&topic_id);

                Ok(())
            }
            ToGossip::JoinPeers(topic_id, peers) => {
                // Convert p2panda public keys to iroh endpoint ids.
                let peers: Vec<EndpointId> = peers
                    .iter()
                    .map(|key: &PublicKey| from_public_key(*key))
                    .collect();

                if let Some(session) = state.sessions.sessions_by_topic_id.get(&topic_id) {
                    let _ = session.cast(ToGossipSession::JoinPeers(peers.clone()));
                }

                Ok(())
            }
            ToGossip::ReceivedMessage {
                bytes,
                delivered_from,
                delivery_scope,
                topic_id,
                session_id: _,
            } => {
                let msg = FromNetwork::ephemeral_message(bytes, delivered_from);

                // Store the delivery scope of the received message.
                state
                    .topic_delivery_scopes
                    .entry(topic_id)
                    .or_default()
                    .push(delivery_scope);

                // Write the received bytes to all subscribers for the associated topic.
                if let Some((_, from_gossip_tx)) = state.sessions.gossip_senders.get(&topic_id) {
                    let _number_of_subscribers = from_gossip_tx.send(msg.clone())?;
                }

                Ok(())
            }
            ToGossip::Joined {
                topic_id,
                peers,
                session_id,
            } => {
                debug!("joined topic {:?} with peers: {:?}", topic_id, peers);

                // Inform the gossip sender actor that the overlay has been joined.
                if let Some(gossip_joined_tx) =
                    state.sessions.gossip_joined_senders.remove(&session_id)
                    && gossip_joined_tx.send(1).is_err()
                {
                    warn!("oneshot gossip joined receiver dropped")
                }

                let peer_set = HashSet::from_iter(peers);

                // Store the neighbours with whom we have joined the topic.
                state.neighbours.insert(topic_id, peer_set);

                Ok(())
            }
            ToGossip::NeighborUp {
                node_id,
                session_id,
            } => {
                // Insert the node into the set of neighbours.
                if let Some(topic_id) = state.sessions.sessions_by_actor_id.get(&session_id)
                    && let Some(neighbours) = state.neighbours.get_mut(topic_id)
                {
                    if let Some(eventually_consistent_streams_actor) = registry::where_is(
                        with_namespace(EVENTUALLY_CONSISTENT_STREAMS, &state.actor_namespace),
                    ) {
                        let actor: ActorRef<ToEventuallyConsistentStreams<E>> =
                            eventually_consistent_streams_actor.into();

                        // Ask the eventually consistent streams actor to initiate a sync session
                        // for this topic.
                        actor.send_message(ToEventuallyConsistentStreams::InitiateSync(
                            *topic_id, node_id,
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
                if let Some(topic_id) = state.sessions.sessions_by_actor_id.get(&session_id)
                    && let Some(neighbours) = state.neighbours.get_mut(topic_id)
                {
                    if let Some(eventually_consistent_streams_actor) = registry::where_is(
                        with_namespace(EVENTUALLY_CONSISTENT_STREAMS, &state.actor_namespace),
                    ) {
                        let actor: ActorRef<ToEventuallyConsistentStreams<E>> =
                            eventually_consistent_streams_actor.into();

                        // Ask the eventually consistent streams actor to end any sync sessions
                        // for this topic.
                        actor.send_message(ToEventuallyConsistentStreams::EndSync(
                            *topic_id, node_id,
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
                if let Some(topic_id) = state.sessions.sessions_by_actor_id.get(&actor_id) {
                    debug!(
                        "gossip actor: received ready from gossip session actor #{} for topic id {:?}",
                        actor_id, topic_id
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if let Some(topic_id) = state.sessions.sessions_by_actor_id.remove(&actor_id) {
                    debug!(
                        "gossip actor: gossip session #{} over topic id {:?} terminated with reason: {:?}",
                        actor_id, topic_id, reason
                    );

                    // Drop all state associated with the terminated gossip session.
                    state.drop_topic_state(&actor_id, &topic_id);
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
                if let Some(topic_id) = state.sessions.sessions_by_actor_id.remove(&actor_id) {
                    warn!(
                        "gossip_actor: gossip session #{} over topic id {:?} failed with reason: {}",
                        actor_id, topic_id, panic_msg
                    );

                    // Drop all state associated with the failed gossip session.
                    state.drop_topic_state(&actor_id, &topic_id);
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::time::Duration;

    use iroh::discovery::EndpointInfo;
    use iroh::discovery::static_provider::StaticProvider;
    use iroh::protocol::Router as IrohRouter;
    use iroh::{Endpoint as IrohEndpoint, RelayMode};
    use iroh_gossip::ALPN as GOSSIP_ALPN;
    use p2panda_core::PrivateKey;
    use p2panda_core::PublicKey;
    use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
    use ractor::{ActorRef, call};
    use tokio::sync::broadcast::error::TryRecvError;
    use tokio::time::sleep;

    use crate::TopicId;
    use crate::actors::address_book::{ADDRESS_BOOK, AddressBook};
    use crate::actors::gossip::session::ToGossipSession;
    use crate::actors::{generate_actor_namespace, with_namespace};
    use crate::network::FromNetwork;
    use crate::test_utils::{test_args, test_args_from_seed};
    use crate::utils::{from_private_key, to_public_key};

    use super::{Gossip, GossipState, ToGossip};

    type TestGossip = Gossip<()>;

    // Use this internal type to introspect the actor's current state.
    pub struct DebugState {
        neighbours: HashMap<TopicId, HashSet<PublicKey>>,
        sessions_by_topic_id: HashMap<TopicId, ActorRef<ToGossipSession>>,
    }

    impl From<&mut GossipState> for DebugState {
        fn from(value: &mut GossipState) -> Self {
            Self {
                neighbours: value.neighbours.clone(),
                sessions_by_topic_id: value.sessions.sessions_by_topic_id.clone(),
            }
        }
    }

    #[tokio::test]
    async fn correct_termination_state() {
        // This test asserts that the state of `sessions_by_topic_id` and `neighbours_by_topic_id`
        // is correctly updated within the `Gossip` actor.
        // Scenario:
        //
        // - Ant joins the gossip topic
        // - Bat joins the gossip topic using ant as bootstrap peer
        // - Cat joins the gossip topic using ant as bootstrap peer
        // - Terminate ant's gossip actor
        // - Assert: Ant's gossip actor state includes the topic that was subscribed to
        // - Assert: Ant's gossip actor state maps the subscribed topic id to the public keys of
        //           bat and cat (neighbours)

        let (ant_args, ant_store, _) = test_args();
        let (bat_args, bat_store, _) = test_args();
        let (cat_args, cat_store, _) = test_args();

        // Create topic id.
        let topic_id = [3; 32];

        // Create keypairs.
        let ant_private_key = ant_args.private_key.clone();
        let bat_private_key = bat_args.private_key.clone();
        let cat_private_key = cat_args.private_key.clone();

        let ant_public_key = ant_private_key.public_key();
        let bat_public_key = bat_private_key.public_key();
        let cat_public_key = cat_private_key.public_key();

        // Create endpoints.
        let ant_discovery = StaticProvider::new();
        let ant_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(ant_private_key))
            .discovery(ant_discovery.clone())
            .bind()
            .await
            .unwrap();

        let bat_discovery = StaticProvider::new();
        let bat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(bat_private_key))
            .discovery(bat_discovery.clone())
            .bind()
            .await
            .unwrap();

        let cat_discovery = StaticProvider::new();
        let cat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(cat_private_key))
            .discovery(cat_discovery.clone())
            .bind()
            .await
            .unwrap();

        // Obtain ant's endpoint information including direct addresses.
        let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_endpoint_info(ant_endpoint_info.clone());

        // Cat discovers ant through some out-of-band process.
        cat_discovery.add_endpoint_info(ant_endpoint_info);

        let thread_pool = ThreadLocalActorSpawner::new();

        // Spawn one address book for each peer.
        let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
        let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
        let cat_actor_namespace = generate_actor_namespace(&cat_args.public_key);

        let (ant_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
            (ant_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();
        let (bat_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
            (bat_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();
        let (cat_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &cat_actor_namespace)),
            (cat_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn gossip actors.
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            TestGossip::spawn(None, ant_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            TestGossip::spawn(None, bat_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();
        let (cat_gossip_actor, cat_gossip_actor_handle) =
            TestGossip::spawn(None, cat_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();

        // Get handles to gossip.
        let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
        let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
        let cat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();

        // Build and spawn routers.
        let ant_router = IrohRouter::builder(ant_endpoint.clone())
            .accept(GOSSIP_ALPN, ant_gossip)
            .spawn();
        let bat_router = IrohRouter::builder(bat_endpoint.clone())
            .accept(GOSSIP_ALPN, bat_gossip)
            .spawn();
        let cat_router = IrohRouter::builder(cat_endpoint.clone())
            .accept(GOSSIP_ALPN, cat_gossip)
            .spawn();

        // Subscribe to the gossip topic.
        let ant_peers = Vec::new();
        let bat_peers = vec![ant_public_key];
        let cat_peers = vec![ant_public_key];

        let (_ant_to_gossip, _ant_from_gossip) =
            call!(ant_gossip_actor, ToGossip::Subscribe, topic_id, ant_peers).unwrap();
        let (_bat_to_gossip, mut _bat_from_gossip) =
            call!(bat_gossip_actor, ToGossip::Subscribe, topic_id, bat_peers).unwrap();
        let (_cat_to_gossip, mut _cat_from_gossip) =
            call!(cat_gossip_actor, ToGossip::Subscribe, topic_id, cat_peers).unwrap();

        // Briefly sleep to allow overlay to form.
        sleep(Duration::from_millis(100)).await;

        // Ensure state expectations are correct for ant's gossip actor.
        let ant_state = call!(ant_gossip_actor, ToGossip::DebugState).unwrap();
        assert!(ant_state.sessions_by_topic_id.contains_key(&topic_id));
        let neighbours = ant_state.neighbours.get(&topic_id).unwrap();
        assert!(neighbours.contains(&bat_public_key));
        assert!(neighbours.contains(&cat_public_key));

        // Stop all other actors and routers.
        ant_gossip_actor.stop(None);
        bat_gossip_actor.stop(None);
        cat_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();
        bat_gossip_actor_handle.await.unwrap();
        cat_gossip_actor_handle.await.unwrap();

        // Stop address book actors.
        ant_address_book_ref.stop(None);
        bat_address_book_ref.stop(None);
        cat_address_book_ref.stop(None);

        ant_router.shutdown().await.unwrap();
        bat_router.shutdown().await.unwrap();
        cat_router.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn two_peer_gossip() {
        // Scenario:
        //
        // - Ant joins the gossip topic
        // - Bat joins the gossip topic using ant as bootstrap peer
        // - Assert: Ant and bat can exchange messages

        let (ant_args, ant_store, _) = test_args();
        let (bat_args, bat_store, _) = test_args();

        // Create topic id.
        let topic_id = [7; 32];

        // Create keypairs.
        let ant_private_key = ant_args.private_key.clone();
        let bat_private_key = bat_args.private_key.clone();

        let ant_public_key = ant_private_key.public_key();
        let bat_public_key = bat_private_key.public_key();

        // Create endpoints.
        let ant_discovery = StaticProvider::new();
        let ant_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(ant_private_key))
            .discovery(ant_discovery.clone())
            .bind()
            .await
            .unwrap();

        let bat_discovery = StaticProvider::new();
        let bat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(bat_private_key))
            .discovery(bat_discovery.clone())
            .bind()
            .await
            .unwrap();

        // Obtain ant's endpoint information including direct addresses.
        let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_endpoint_info(ant_endpoint_info);

        let thread_pool = ThreadLocalActorSpawner::new();

        // Spawn one address book for each peer.
        let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
        let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);

        let (ant_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
            (ant_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();
        let (bat_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
            (bat_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn gossip actors.
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            TestGossip::spawn(None, ant_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            TestGossip::spawn(None, bat_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();

        // Get handles to gossip.
        let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
        let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();

        // Build and spawn routers.
        let ant_router = IrohRouter::builder(ant_endpoint.clone())
            .accept(GOSSIP_ALPN, ant_gossip)
            .spawn();
        let bat_router = IrohRouter::builder(bat_endpoint.clone())
            .accept(GOSSIP_ALPN, bat_gossip)
            .spawn();

        // Subscribe to the gossip topic.
        let ant_peers = Vec::new();
        let bat_peers = vec![ant_public_key];

        let (ant_to_gossip, ant_from_gossip) =
            call!(ant_gossip_actor, ToGossip::Subscribe, topic_id, ant_peers).unwrap();
        let (bat_to_gossip, bat_from_gossip) =
            call!(bat_gossip_actor, ToGossip::Subscribe, topic_id, bat_peers).unwrap();

        // Briefly sleep to allow overlay to form.
        sleep(Duration::from_millis(100)).await;

        // Subscribe to sender to obtain receiver.
        let mut bat_from_gossip_rx = bat_from_gossip.subscribe();
        let mut ant_from_gossip_rx = ant_from_gossip.subscribe();

        // Send message from ant to bat.
        let ant_msg_to_bat = b"hi bat!".to_vec();
        ant_to_gossip.send(ant_msg_to_bat.clone()).await.unwrap();

        // Ensure bat receives the message from ant.
        let Ok(msg) = bat_from_gossip_rx.recv().await else {
            panic!("expected msg from ant")
        };

        assert_eq!(
            msg,
            FromNetwork::ephemeral_message(ant_msg_to_bat, ant_public_key)
        );

        // Send message from bat to ant.
        let bat_msg_to_ant = b"oh hey ant!".to_vec();
        bat_to_gossip.send(bat_msg_to_ant.clone()).await.unwrap();

        // Ensure ant receives the message from bat.
        let Ok(msg) = ant_from_gossip_rx.recv().await else {
            panic!("expected msg from bat")
        };

        assert_eq!(
            msg,
            FromNetwork::ephemeral_message(bat_msg_to_ant, bat_public_key)
        );

        // Stop gossip actors.
        ant_gossip_actor.stop(None);
        bat_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();
        bat_gossip_actor_handle.await.unwrap();

        // Stop address book actors.
        ant_address_book_ref.stop(None);
        bat_address_book_ref.stop(None);

        // Shutdown routers.
        bat_router.shutdown().await.unwrap();
        ant_router.shutdown().await.unwrap();
    }

    // @TODO: This test keeps hanging at random times.
    #[ignore]
    #[tokio::test]
    async fn third_peer_joins_non_bootstrap() {
        // Scenario:
        //
        // - Ant joins the gossip topic
        // - Bat joins the gossip topic using ant as bootstrap peer
        // - Cat joins the gossip topic using bat as bootstrap peer
        // - Assert: Ant, bat and cat can exchange messages

        let (ant_args, ant_store, _) = test_args();
        let (bat_args, bat_store, _) = test_args();
        let (cat_args, cat_store, _) = test_args();

        // Create topic id.
        let topic_id = [11; 32];

        // Create keypairs.
        let ant_private_key = ant_args.private_key.clone();
        let bat_private_key = bat_args.private_key.clone();
        let cat_private_key = cat_args.private_key.clone();

        let ant_public_key = ant_private_key.public_key();
        let bat_public_key = bat_private_key.public_key();
        let cat_public_key = cat_private_key.public_key();

        // Create endpoints.
        let ant_discovery = StaticProvider::new();
        let ant_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(ant_private_key))
            .discovery(ant_discovery.clone())
            .bind()
            .await
            .unwrap();

        let bat_discovery = StaticProvider::new();
        let bat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(bat_private_key))
            .discovery(bat_discovery.clone())
            .bind()
            .await
            .unwrap();

        let cat_discovery = StaticProvider::new();
        let cat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(cat_private_key))
            .discovery(cat_discovery.clone())
            .bind()
            .await
            .unwrap();

        // Obtain ant's endpoint information including direct addresses.
        let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_endpoint_info(ant_endpoint_info);

        let thread_pool = ThreadLocalActorSpawner::new();

        let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
        let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
        let cat_actor_namespace = generate_actor_namespace(&cat_args.public_key);

        let (ant_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
            (ant_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();
        let (bat_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
            (bat_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();
        let (cat_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &cat_actor_namespace)),
            (cat_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn gossip actors.
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            TestGossip::spawn(None, ant_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            TestGossip::spawn(None, bat_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();
        let (cat_gossip_actor, cat_gossip_actor_handle) =
            TestGossip::spawn(None, cat_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();

        // Get handles to gossip.
        let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
        let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
        let cat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();

        // Build and spawn routers.
        let ant_router = IrohRouter::builder(ant_endpoint.clone())
            .accept(GOSSIP_ALPN, ant_gossip)
            .spawn();
        let bat_router = IrohRouter::builder(bat_endpoint.clone())
            .accept(GOSSIP_ALPN, bat_gossip)
            .spawn();
        let cat_router = IrohRouter::builder(cat_endpoint.clone())
            .accept(GOSSIP_ALPN, cat_gossip)
            .spawn();

        // Subscribe to the gossip topic.
        let ant_peers = Vec::new();
        let bat_peers = vec![ant_public_key];

        let (ant_to_gossip, _ant_from_gossip) =
            call!(ant_gossip_actor, ToGossip::Subscribe, topic_id, ant_peers).unwrap();
        let (_bat_to_gossip, bat_from_gossip) =
            call!(bat_gossip_actor, ToGossip::Subscribe, topic_id, bat_peers).unwrap();

        // Briefly sleep to allow overlay to form.
        sleep(Duration::from_millis(250)).await;

        // Subscribe to sender to obtain receiver.
        let mut bat_from_gossip_rx = bat_from_gossip.subscribe();

        // Obtain bat's endpoint information including direct addresses.
        let bat_endpoint_info: EndpointInfo = bat_endpoint.addr().into();

        // Cat discovers bat through some out-of-band process.
        cat_discovery.add_endpoint_info(bat_endpoint_info);

        let cat_peers = vec![bat_public_key];

        // Cat subscribes to topic using bat as bootstrap.
        let (cat_to_gossip, cat_from_gossip) =
            call!(cat_gossip_actor, ToGossip::Subscribe, topic_id, cat_peers).unwrap();

        // Briefly sleep to allow overlay to form.
        sleep(Duration::from_millis(250)).await;

        let mut cat_from_gossip_rx = cat_from_gossip.subscribe();

        // Send message from cat to ant and bat.
        let cat_msg_to_ant_and_bat = b"hi ant and bat!".to_vec();
        cat_to_gossip
            .send(cat_msg_to_ant_and_bat.clone())
            .await
            .unwrap();

        // Ensure bat receives cat's message.
        let Ok(msg) = bat_from_gossip_rx.recv().await else {
            panic!("expected msg from cat")
        };

        assert_eq!(
            msg,
            FromNetwork::ephemeral_message(cat_msg_to_ant_and_bat, cat_public_key)
        );

        // Send message from ant to bat and cat.
        let ant_msg_to_bat_and_cat = b"hi bat and cat!".to_vec();
        ant_to_gossip
            .send(ant_msg_to_bat_and_cat.clone())
            .await
            .unwrap();

        // Ensure cat receives ant's message.
        let Ok(msg) = cat_from_gossip_rx.recv().await else {
            panic!("expected msg from ant")
        };

        // NOTE: In this case the message is delivered by bat; not directly from ant.
        assert_eq!(
            msg,
            FromNetwork::ephemeral_message(ant_msg_to_bat_and_cat, bat_public_key)
        );

        // Stop gossip actors.
        ant_gossip_actor.stop(None);
        bat_gossip_actor.stop(None);
        cat_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();
        bat_gossip_actor_handle.await.unwrap();
        cat_gossip_actor_handle.await.unwrap();

        // Stop address book actors.
        ant_address_book_ref.stop(None);
        bat_address_book_ref.stop(None);
        cat_address_book_ref.stop(None);

        // Shutdown routers.
        ant_router.shutdown().await.unwrap();
        bat_router.shutdown().await.unwrap();
        cat_router.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn three_peer_gossip_with_rejoin() {
        // Scenario:
        //
        // - Ant joins the gossip topic
        // - Bat joins the gossip topic using ant as bootstrap peer
        // - Assert: Ant and bat can exchange messages
        // - Ant goes offline
        // - Cat joins the gossip topic using ant as bootstrap peer
        // - Assert: Bat and cat can't exchange messages (proof of partition)
        // - Cat learns about bat through out-of-band discovery process
        // - Cat joins bat on established gossip topic
        // - Assert: Bat and cat can now exchange messages (proof of healed partition)

        let (ant_args, ant_store, _) = test_args();
        let (bat_args, bat_store, _) = test_args();
        let (cat_args, cat_store, _) = test_args();

        // Create topic id.
        let topic_id = [9; 32];

        // Create keypairs.
        let ant_private_key = ant_args.private_key.clone();
        let bat_private_key = bat_args.private_key.clone();
        let cat_private_key = cat_args.private_key.clone();

        let ant_public_key = ant_private_key.public_key();
        let bat_public_key = bat_private_key.public_key();
        let cat_public_key = cat_private_key.public_key();

        // Create endpoints.
        let ant_discovery = StaticProvider::new();
        let ant_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(ant_private_key))
            .discovery(ant_discovery.clone())
            .bind()
            .await
            .unwrap();

        let bat_discovery = StaticProvider::new();
        let bat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(bat_private_key))
            .discovery(bat_discovery.clone())
            .bind()
            .await
            .unwrap();

        let cat_discovery = StaticProvider::new();
        let cat_endpoint = IrohEndpoint::empty_builder(RelayMode::Disabled)
            .secret_key(from_private_key(cat_private_key))
            .discovery(cat_discovery.clone())
            .bind()
            .await
            .unwrap();

        // Obtain ant's endpoint information including direct addresses.
        let ant_endpoint_info: EndpointInfo = ant_endpoint.addr().into();

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_endpoint_info(ant_endpoint_info);

        let thread_pool = ThreadLocalActorSpawner::new();

        // Spawn one address book for each peer.
        let ant_actor_namespace = generate_actor_namespace(&ant_args.public_key);
        let bat_actor_namespace = generate_actor_namespace(&bat_args.public_key);
        let cat_actor_namespace = generate_actor_namespace(&cat_args.public_key);

        let (ant_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &ant_actor_namespace)),
            (ant_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();
        let (bat_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &bat_actor_namespace)),
            (bat_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();
        let (cat_address_book_ref, _) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &cat_actor_namespace)),
            (cat_store.clone(),),
            thread_pool.clone(),
        )
        .await
        .unwrap();

        // Spawn gossip actors.
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            TestGossip::spawn(None, ant_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            TestGossip::spawn(None, bat_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();
        let (cat_gossip_actor, cat_gossip_actor_handle) =
            TestGossip::spawn(None, cat_endpoint.clone(), thread_pool.clone())
                .await
                .unwrap();

        // Get handles to gossip.
        let ant_gossip = call!(ant_gossip_actor, ToGossip::Handle).unwrap();
        let bat_gossip = call!(bat_gossip_actor, ToGossip::Handle).unwrap();
        let cat_gossip = call!(cat_gossip_actor, ToGossip::Handle).unwrap();

        // Build and spawn routers.
        let ant_router = IrohRouter::builder(ant_endpoint.clone())
            .accept(GOSSIP_ALPN, ant_gossip)
            .spawn();
        let bat_router = IrohRouter::builder(bat_endpoint.clone())
            .accept(GOSSIP_ALPN, bat_gossip)
            .spawn();
        let cat_router = IrohRouter::builder(cat_endpoint.clone())
            .accept(GOSSIP_ALPN, cat_gossip)
            .spawn();

        // Ant and bat subscribe to the gossip topic.
        let ant_peers = Vec::new();
        let bat_peers = vec![ant_public_key];

        let (ant_to_gossip, ant_from_gossip) =
            call!(ant_gossip_actor, ToGossip::Subscribe, topic_id, ant_peers).unwrap();
        let (bat_to_gossip, bat_from_gossip) =
            call!(bat_gossip_actor, ToGossip::Subscribe, topic_id, bat_peers).unwrap();

        // Subscribe to sender to obtain receiver.
        let mut bat_from_gossip_rx = bat_from_gossip.subscribe();
        let mut ant_from_gossip_rx = ant_from_gossip.subscribe();

        // Send message from ant to bat.
        let ant_msg_to_bat = b"hi bat!".to_vec();
        ant_to_gossip.send(ant_msg_to_bat.clone()).await.unwrap();

        // Ensure bat receives the message from ant.
        let Ok(msg) = bat_from_gossip_rx.recv().await else {
            panic!("expected msg from ant")
        };

        assert_eq!(
            msg,
            FromNetwork::ephemeral_message(ant_msg_to_bat, ant_public_key)
        );

        // Send message from bat to ant.
        let bat_msg_to_ant = b"oh hey ant!".to_vec();
        bat_to_gossip.send(bat_msg_to_ant.clone()).await.unwrap();

        // Ensure ant receives the message from bat.
        let Ok(msg) = ant_from_gossip_rx.recv().await else {
            panic!("expected msg from bat")
        };

        assert_eq!(
            msg,
            FromNetwork::ephemeral_message(bat_msg_to_ant, bat_public_key)
        );

        // Stop the gossip actor and router for ant (going offline).
        ant_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();
        ant_router.shutdown().await.unwrap();

        // Cat joins the gossip topic (using ant as bootstrap).
        let cat_peers = vec![ant_public_key];

        let (cat_to_gossip, cat_from_gossip) =
            call!(cat_gossip_actor, ToGossip::Subscribe, topic_id, cat_peers).unwrap();

        let mut cat_from_gossip_rx = cat_from_gossip.subscribe();

        // Send message from cat to bat.
        let cat_msg_to_bat = b"hi bat!".to_vec();
        cat_to_gossip.send(cat_msg_to_bat.clone()).await.unwrap();

        // Briefly sleep to allow processing of sent message.
        sleep(Duration::from_millis(50)).await;

        // Ensure bat has not received the message from cat.
        assert_eq!(bat_from_gossip_rx.try_recv(), Err(TryRecvError::Empty));

        // Send message from bat to cat.
        let bat_msg_to_cat = b"anyone out there?".to_vec();
        bat_to_gossip.send(bat_msg_to_cat.clone()).await.unwrap();

        // Briefly sleep to allow processing of sent message.
        sleep(Duration::from_millis(50)).await;

        // Ensure cat has not received the message from bat.
        assert_eq!(cat_from_gossip_rx.try_recv(), Err(TryRecvError::Empty));

        // At this point we have proof of partition; bat and cat are subscribed to the same gossip
        // topic but cannot "hear" one another.

        // Obtain bat's endpoint information including direct addresses.
        let bat_endpoint_info: EndpointInfo = bat_endpoint.addr().into();

        // Cat discovers bat through some out-of-band process.
        cat_discovery.add_endpoint_info(bat_endpoint_info);

        // Cat explicitly joins bat on the gossip topic.
        let _ = cat_gossip_actor.cast(ToGossip::JoinPeers(topic_id, vec![bat_public_key]));

        // Send message from cat to bat.
        let cat_msg_to_bat = b"you there bat?".to_vec();
        cat_to_gossip.send(cat_msg_to_bat.clone()).await.unwrap();

        // Briefly sleep to allow processing of sent message.
        sleep(Duration::from_millis(50)).await;

        // Ensure bat receives the message from cat.
        let Ok(msg) = bat_from_gossip_rx.recv().await else {
            panic!("expected msg from cat")
        };

        assert_eq!(
            msg,
            FromNetwork::ephemeral_message(cat_msg_to_bat, cat_public_key)
        );

        // Send message from bat to cat.
        let bat_msg_to_cat = b"yoyo!".to_vec();
        bat_to_gossip.send(bat_msg_to_cat.clone()).await.unwrap();

        // Briefly sleep to allow processing of sent message.
        sleep(Duration::from_millis(500)).await;

        // Ensure cat receives the message from bat.
        let Ok(msg) = cat_from_gossip_rx.recv().await else {
            panic!("expected msg from bat")
        };

        assert_eq!(
            msg,
            FromNetwork::ephemeral_message(bat_msg_to_cat, bat_public_key)
        );

        // Stop gossip actors.
        bat_gossip_actor.stop(None);
        bat_gossip_actor_handle.await.unwrap();
        cat_gossip_actor.stop(None);
        cat_gossip_actor_handle.await.unwrap();

        // Stop address book actors.
        ant_address_book_ref.stop(None);
        bat_address_book_ref.stop(None);
        cat_address_book_ref.stop(None);

        // Shutdown routers.
        bat_router.shutdown().await.unwrap();
        cat_router.shutdown().await.unwrap();
    }
}
