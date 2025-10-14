// SPDX-License-Identifier: MIT OR Apache-2.0

//! An `iroh`-specific gossip actor for message broadcast.
mod joiner;
mod listener;
mod receiver;
mod sender;
mod session;

use std::collections::{HashMap, HashSet};

use iroh::Endpoint as IrohEndpoint;
use iroh::NodeId;
use iroh_gossip::net::Gossip as IrohGossip;
use iroh_gossip::proto::{Config as IrohGossipConfig, DeliveryScope as IrohDeliveryScope};
use p2panda_core::PublicKey;
use ractor::{
    Actor, ActorId, ActorProcessingErr, ActorRef, Message, RpcReplyPort, SupervisionEvent,
};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::oneshot::{self, Sender as OneshotSender};
use tracing::{debug, warn};

use crate::actors::gossip::session::{GossipSession, ToGossipSession};
use crate::{TopicId, from_public_key};

/// Bytes received from gossip, along with the public key of the peer from whom the message was
/// received. Note that the delivering peer is not necessarily the author of the bytes.
pub type MsgBytesAndDeliverer = (Vec<u8>, PublicKey);

// TODO: Remove once used.
#[allow(dead_code)]
pub enum ToGossip {
    /// Return a handle to the iroh gossip actor.
    ///
    /// This is required when registering the gossip ALPN with the router.
    Handle(RpcReplyPort<IrohGossip>),

    /// Subscribe to the given topic, using the given peers as gossip bootstrap nodes.
    Subscribe(
        TopicId,
        Vec<PublicKey>,
        RpcReplyPort<(Sender<Vec<u8>>, Receiver<MsgBytesAndDeliverer>)>,
    ),

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
        peer: PublicKey,
        session_id: ActorId,
    },

    /// Lost a direct neighbor in the gossip overlay.
    NeighborDown {
        peer: PublicKey,
        session_id: ActorId,
    },

    /// Received a message from the gossip overlay.
    ReceivedMessage {
        bytes: Vec<u8>,
        delivered_from: PublicKey,
        delivery_scope: IrohDeliveryScope,
        topic_id: TopicId,
        // TODO: Remove once used.
        #[allow(dead_code)]
        session_id: ActorId,
    },
}

impl Message for ToGossip {}

pub struct GossipState {
    gossip: IrohGossip,
    sessions_by_actor_id: HashMap<ActorId, TopicId>,
    sessions_by_topic_id: HashMap<TopicId, Vec<ActorRef<ToGossipSession>>>,
    neighbours_by_topic_id: HashMap<TopicId, HashSet<PublicKey>>,
    from_gossip_senders: HashMap<TopicId, Vec<Sender<MsgBytesAndDeliverer>>>,
    gossip_joined_senders: HashMap<ActorId, OneshotSender<u8>>,
    topic_delivery_scopes: HashMap<TopicId, Vec<IrohDeliveryScope>>,
}

pub struct Gossip;

impl Actor for Gossip {
    type State = GossipState;
    type Msg = ToGossip;
    // TODO: Pass in any required config.
    type Arguments = IrohEndpoint;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        endpoint: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let config = IrohGossipConfig::default();

        let gossip = IrohGossip::builder()
            .max_message_size(config.max_message_size)
            .membership_config(config.membership)
            .broadcast_config(config.broadcast)
            .spawn(endpoint);

        let sessions_by_actor_id = HashMap::new();
        let sessions_by_topic_id = HashMap::new();
        let neighbours_by_topic_id = HashMap::new();
        let from_gossip_senders = HashMap::new();
        let gossip_joined_senders = HashMap::new();
        let topic_delivery_scopes = HashMap::new();

        // TODO: The router needs to be configured to accept gossip protocol.
        // This needs to be done when the router is built.
        // Consider how to do this via config.

        let state = GossipState {
            gossip,
            sessions_by_actor_id,
            sessions_by_topic_id,
            neighbours_by_topic_id,
            from_gossip_senders,
            gossip_joined_senders,
            topic_delivery_scopes,
        };

        Ok(state)
    }

    async fn post_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Leave all subscribed topics, send `Disconnect` messages to peers and drop all state
        // and connections.
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

                // Don't respond if the receiver has been dropped.
                if !reply.is_closed() {
                    let _ = reply.send(gossip);
                }

                Ok(())
            }
            ToGossip::Subscribe(topic_id, peers, reply) => {
                // TODO: How do we handle a subscribe for a topic that already has an active
                // subscription? Either we multiplex over a subscription or we establish an
                // additional session. Right now I (glyph) am leaning towards establishing
                // additional sessions.

                // Channel to receive messages from the user (to the gossip overlay).
                let (to_gossip_tx, to_gossip_rx) = mpsc::channel(128);
                // Channel to receive messages from the gossip overlay (to the user).
                let (from_gossip_tx, from_gossip_rx) = mpsc::channel(128);

                // Oneshot channel to notify the session sender(s) that the overlay has been
                // joined.
                let (gossip_joined_tx, gossip_joined_rx) = oneshot::channel();

                // Convert p2panda public keys to iroh node ids.
                let peers = peers
                    .iter()
                    .map(|key: &PublicKey| from_public_key(*key))
                    .collect();

                // Subscribe to the gossip topic (without waiting for a connection).
                let subscription = state.gossip.subscribe(topic_id.into(), peers).await?;

                // TODO: Store a clone of the `to_gossip_rx` channel to allow for recovery if the session
                // fails. This will likely be handled in a higher-level `SubscriptionActor`. We'll
                // need to use an MPMC channel (e.g. from the `async_channel` crate).

                // Spawn the session actor with the gossip topic subscription.
                let (gossip_session_actor, _) = Actor::spawn_linked(
                    None,
                    GossipSession::new(myself.clone()),
                    (topic_id, subscription, to_gossip_rx, gossip_joined_rx),
                    myself.clone().into(),
                )
                .await?;

                // Associate the session actor id with the topic.
                let gossip_session_actor_id = gossip_session_actor.get_id();
                let _ = state
                    .sessions_by_actor_id
                    .insert(gossip_session_actor_id, topic_id);

                // Associate the session actor with the gossip joined sender.
                let _ = state
                    .gossip_joined_senders
                    .insert(gossip_session_actor_id, gossip_joined_tx);

                // Associate the topic id with the session actor.
                state
                    .sessions_by_topic_id
                    .entry(topic_id)
                    .or_default()
                    .push(gossip_session_actor);

                // Associate the user channel (sender) with the topic.
                state
                    .from_gossip_senders
                    .entry(topic_id)
                    .or_default()
                    .push(from_gossip_tx);

                // Return sender / receiver pair to the user.
                if !reply.is_closed() {
                    // TODO: Handle case where receiver channel has been dropped.
                    let _ = reply.send((to_gossip_tx, from_gossip_rx));
                }

                Ok(())
            }
            ToGossip::JoinPeers(topic_id, peers) => {
                // Convert p2panda public keys to iroh node ids.
                let peers: Vec<NodeId> = peers
                    .iter()
                    .map(|key: &PublicKey| from_public_key(*key))
                    .collect();

                if let Some(sessions) = state.sessions_by_topic_id.get(&topic_id) {
                    for session in sessions {
                        let _ = session.cast(ToGossipSession::JoinPeers(peers.clone()));
                    }
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
                let msg = (bytes, delivered_from);

                // Store the delivery scope of the received message.
                state
                    .topic_delivery_scopes
                    .entry(topic_id)
                    .or_default()
                    .push(delivery_scope);

                // Write the received bytes to all subscribers for the associated topic.
                if let Some(senders) = state.from_gossip_senders.get(&topic_id) {
                    for sender in senders {
                        // TODO: We need to tidy up properly when the receiver is dropped.
                        sender.send(msg.clone()).await?
                    }
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
                if let Some(gossip_joined_tx) = state.gossip_joined_senders.remove(&session_id)
                    && gossip_joined_tx.send(1).is_err()
                {
                    warn!("oneshot gossip joined receiver dropped")
                }

                let peer_set = HashSet::from_iter(peers);

                // Store the neighbours with whom we have joined the topic.
                state.neighbours_by_topic_id.insert(topic_id, peer_set);

                Ok(())
            }
            ToGossip::NeighborUp { peer, session_id } => {
                // Insert the peer into the set of neighbours.
                if let Some(topic_id) = state.sessions_by_actor_id.get(&session_id)
                    && let Some(peer_set) = state.neighbours_by_topic_id.get_mut(topic_id)
                {
                    peer_set.insert(peer);
                }

                Ok(())
            }
            ToGossip::NeighborDown { peer, session_id } => {
                // Remove the peer from the set of neighbours.
                if let Some(topic_id) = state.sessions_by_actor_id.get(&session_id)
                    && let Some(peer_set) = state.neighbours_by_topic_id.get_mut(topic_id)
                {
                    peer_set.remove(&peer);
                }

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
                if let Some(topic_id) = state.sessions_by_actor_id.get(&actor_id) {
                    debug!(
                        "gossip actor: received ready from gossip session actor #{} for topic id {:?}",
                        actor_id, topic_id
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if let Some(topic_id) = state.sessions_by_actor_id.remove(&actor_id) {
                    debug!(
                        "gossip actor: gossip session #{} over topic id {:?} terminated with reason: {:?}",
                        actor_id, topic_id, reason
                    );

                    // Drop all state associated with the terminated gossip session.
                    if let Some(gossip_session_actor) = state.sessions_by_topic_id.remove(&topic_id)
                    {
                        drop(gossip_session_actor)
                    }
                    if let Some(neighbours) = state.neighbours_by_topic_id.remove(&topic_id) {
                        drop(neighbours)
                    }
                    if let Some(from_gossip_tx) = state.from_gossip_senders.remove(&topic_id) {
                        drop(from_gossip_tx)
                    }
                    if let Some(gossip_joined_tx) = state.gossip_joined_senders.remove(&actor_id) {
                        drop(gossip_joined_tx)
                    }
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
                if let Some(topic_id) = state.sessions_by_actor_id.remove(&actor_id) {
                    warn!(
                        "gossip_actor: gossip session #{} over topic id {:?} failed with reason: {}",
                        actor_id, topic_id, panic_msg
                    );

                    // Drop all state associated with the failed gossip session.
                    if let Some(gossip_session_actor) = state.sessions_by_topic_id.remove(&topic_id)
                    {
                        drop(gossip_session_actor)
                    }
                    if let Some(neighbours) = state.neighbours_by_topic_id.remove(&topic_id) {
                        drop(neighbours)
                    }
                    if let Some(from_gossip_tx) = state.from_gossip_senders.remove(&topic_id) {
                        drop(from_gossip_tx)
                    }
                    if let Some(gossip_joined_tx) = state.gossip_joined_senders.remove(&actor_id) {
                        drop(gossip_joined_tx)
                    }
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use iroh::Endpoint as IrohEndpoint;
    use iroh::discovery::NodeInfo;
    use iroh::discovery::static_provider::StaticProvider;
    use iroh::protocol::Router as IrohRouter;
    use iroh_gossip::ALPN as GOSSIP_ALPN;
    use p2panda_core::PrivateKey;
    use ractor::{Actor, call};
    use tokio::sync::mpsc::error::TryRecvError;
    use tokio::sync::oneshot;
    use tokio::time::sleep;

    use crate::actors::test_utils::{ActorResult, TestSupervisor};
    use crate::{from_private_key, from_public_key};

    use super::{Gossip, GossipState, ToGossip};

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

        // Create topic id.
        let topic_id = [3; 32];

        // Create keypairs.
        let ant_private_key = PrivateKey::new();
        let bat_private_key = PrivateKey::new();
        let cat_private_key = PrivateKey::new();

        let ant_public_key = ant_private_key.public_key();
        let bat_public_key = bat_private_key.public_key();
        let cat_public_key = cat_private_key.public_key();

        // Create endpoints.
        let ant_discovery = StaticProvider::new();
        let ant_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(ant_private_key))
            .add_discovery(ant_discovery.clone())
            .bind()
            .await
            .unwrap();

        let bat_discovery = StaticProvider::new();
        let bat_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(bat_private_key))
            .add_discovery(bat_discovery.clone())
            .bind()
            .await
            .unwrap();

        let cat_discovery = StaticProvider::new();
        let cat_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(cat_private_key))
            .add_discovery(cat_discovery.clone())
            .bind()
            .await
            .unwrap();

        // Obtain ant's node information including direct addresses.
        let ant_addrs = ant_endpoint.node_addr();
        let ant_node_info = NodeInfo::new(from_public_key(ant_public_key))
            .with_direct_addresses(ant_addrs.direct_addresses);

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_node_info(ant_node_info.clone());

        // Cat discovers ant through some out-of-band process.
        cat_discovery.add_node_info(ant_node_info);

        // Spawn gossip actors.
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            Actor::spawn(None, Gossip, ant_endpoint.clone())
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            Actor::spawn(None, Gossip, bat_endpoint.clone())
                .await
                .unwrap();
        let (cat_gossip_actor, cat_gossip_actor_handle) =
            Actor::spawn(None, Gossip, cat_endpoint.clone())
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

        // Spawn a test supervisor actor.
        let (ant_supervisor_tx, ant_supervisor_rx) = oneshot::channel();
        let (ant_supervisor_actor, ant_supervisor_actor_handle) =
            Actor::spawn(None, TestSupervisor, ant_supervisor_tx)
                .await
                .unwrap();

        // Link ant's gossip actor to the test supervisor.
        ant_gossip_actor.link(ant_supervisor_actor.into());

        // Briefly sleep to allow overlay to form.
        sleep(Duration::from_millis(100)).await;

        // Stop ant's actors and router.
        ant_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();

        ant_router.shutdown().await.unwrap();

        // Get the termination result from ant's supervisor actor.
        let Ok(ant_gossip_actor_result) = ant_supervisor_rx.await else {
            panic!("expected result from gossip actor")
        };
        let ActorResult::Terminated(state, _reason) = ant_gossip_actor_result else {
            panic!("expected clean termination of gossip actor")
        };
        let Some(mut boxed_state) = state else {
            panic!("expected state to be returned from terminated gossip actor")
        };

        // Ensure state expectations are correct for ant's gossip actor.
        if let Ok(state) = boxed_state.take::<GossipState>() {
            assert!(state.sessions_by_topic_id.contains_key(&topic_id));

            let neighbours = state.neighbours_by_topic_id.get(&topic_id).unwrap();
            assert!(neighbours.contains(&bat_public_key));
            assert!(neighbours.contains(&cat_public_key));
        }

        // Stop all other actors and routers.
        bat_gossip_actor.stop(None);
        cat_gossip_actor.stop(None);
        bat_gossip_actor_handle.await.unwrap();
        cat_gossip_actor_handle.await.unwrap();
        ant_supervisor_actor_handle.await.unwrap();

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

        // Create topic id.
        let topic_id = [7; 32];

        // Create keypairs.
        let ant_private_key = PrivateKey::new();
        let bat_private_key = PrivateKey::new();

        let ant_public_key = ant_private_key.public_key();
        let bat_public_key = bat_private_key.public_key();

        // Create endpoints.
        let ant_discovery = StaticProvider::new();
        let ant_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(ant_private_key))
            .add_discovery(ant_discovery.clone())
            .bind()
            .await
            .unwrap();

        let bat_discovery = StaticProvider::new();
        let bat_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(bat_private_key))
            .add_discovery(bat_discovery.clone())
            .bind()
            .await
            .unwrap();

        // Obtain ant's node information including direct addresses.
        let ant_addrs = ant_endpoint.node_addr();
        let ant_node_info = NodeInfo::new(from_public_key(ant_public_key))
            .with_direct_addresses(ant_addrs.direct_addresses);

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_node_info(ant_node_info);

        // Spawn gossip actors.
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            Actor::spawn(None, Gossip, ant_endpoint.clone())
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            Actor::spawn(None, Gossip, bat_endpoint.clone())
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

        let (ant_to_gossip, mut ant_from_gossip) =
            call!(ant_gossip_actor, ToGossip::Subscribe, topic_id, ant_peers).unwrap();
        let (bat_to_gossip, mut bat_from_gossip) =
            call!(bat_gossip_actor, ToGossip::Subscribe, topic_id, bat_peers).unwrap();

        // Send message from ant to bat.
        let ant_msg_to_bat = b"hi bat!".to_vec();
        ant_to_gossip.send(ant_msg_to_bat.clone()).await.unwrap();

        // Ensure bat receives the message from ant.
        let Some(msg) = bat_from_gossip.recv().await else {
            panic!("expected msg from ant")
        };

        assert_eq!(msg, (ant_msg_to_bat, ant_public_key));

        // Send message from bat to ant.
        let bat_msg_to_ant = b"oh hey ant!".to_vec();
        bat_to_gossip.send(bat_msg_to_ant.clone()).await.unwrap();

        // Ensure ant receives the message from bat.
        let Some(msg) = ant_from_gossip.recv().await else {
            panic!("expected msg from bat")
        };

        assert_eq!(msg, (bat_msg_to_ant, bat_public_key));

        // Stop gossip actors.
        ant_gossip_actor.stop(None);
        bat_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();
        bat_gossip_actor_handle.await.unwrap();

        // Shutdown routers.
        bat_router.shutdown().await.unwrap();
        ant_router.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn third_peer_joins_non_bootstrap() {
        // Scenario:
        //
        // - Ant joins the gossip topic
        // - Bat joins the gossip topic using ant as bootstrap peer
        // - Cat joins the gossip topic using bat as bootstrap peer
        // - Assert: Ant, bat and cat can exchange messages

        // Create topic id.
        let topic_id = [11; 32];

        // Create keypairs.
        let ant_private_key = PrivateKey::new();
        let bat_private_key = PrivateKey::new();
        let cat_private_key = PrivateKey::new();

        let ant_public_key = ant_private_key.public_key();
        let bat_public_key = bat_private_key.public_key();
        let cat_public_key = cat_private_key.public_key();

        // Create endpoints.
        let ant_discovery = StaticProvider::new();
        let ant_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(ant_private_key))
            .add_discovery(ant_discovery.clone())
            .bind()
            .await
            .unwrap();

        let bat_discovery = StaticProvider::new();
        let bat_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(bat_private_key))
            .add_discovery(bat_discovery.clone())
            .bind()
            .await
            .unwrap();

        let cat_discovery = StaticProvider::new();
        let cat_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(cat_private_key))
            .add_discovery(cat_discovery.clone())
            .bind()
            .await
            .unwrap();

        // Obtain ant's node information including direct addresses.
        let ant_addrs = ant_endpoint.node_addr();
        let ant_node_info = NodeInfo::new(from_public_key(ant_public_key))
            .with_direct_addresses(ant_addrs.direct_addresses);

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_node_info(ant_node_info);

        // Spawn gossip actors.
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            Actor::spawn(None, Gossip, ant_endpoint.clone())
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            Actor::spawn(None, Gossip, bat_endpoint.clone())
                .await
                .unwrap();
        let (cat_gossip_actor, cat_gossip_actor_handle) =
            Actor::spawn(None, Gossip, cat_endpoint.clone())
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
        let (_bat_to_gossip, mut bat_from_gossip) =
            call!(bat_gossip_actor, ToGossip::Subscribe, topic_id, bat_peers).unwrap();

        // Obtain bat's node information including direct addresses.
        let bat_addrs = bat_endpoint.node_addr();
        let bat_node_info = NodeInfo::new(from_public_key(bat_public_key))
            .with_direct_addresses(bat_addrs.direct_addresses);

        // Cat discovers bat through some out-of-band process.
        cat_discovery.add_node_info(bat_node_info);

        let cat_peers = vec![bat_public_key];

        // Cat subscribes to topic using bat as bootstrap.
        let (cat_to_gossip, mut cat_from_gossip) =
            call!(cat_gossip_actor, ToGossip::Subscribe, topic_id, cat_peers).unwrap();

        // Send message from cat to ant and bat.
        let cat_msg_to_ant_and_bat = b"hi ant and bat!".to_vec();
        cat_to_gossip
            .send(cat_msg_to_ant_and_bat.clone())
            .await
            .unwrap();

        // Ensure bat receives cat's message.
        let Some(msg) = bat_from_gossip.recv().await else {
            panic!("expected msg from cat")
        };

        assert_eq!(msg, (cat_msg_to_ant_and_bat, cat_public_key));

        // Send message from ant to bat and cat.
        let ant_msg_to_bat_and_cat = b"hi bat and cat!".to_vec();
        ant_to_gossip
            .send(ant_msg_to_bat_and_cat.clone())
            .await
            .unwrap();

        // Ensure cat receives ant's message.
        let Some(msg) = cat_from_gossip.recv().await else {
            panic!("expected msg from ant")
        };

        // NOTE: In this case the message is delivered by bat; not directly from ant.
        assert_eq!(msg, (ant_msg_to_bat_and_cat, bat_public_key));

        // Stop gossip actors.
        ant_gossip_actor.stop(None);
        bat_gossip_actor.stop(None);
        cat_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();
        bat_gossip_actor_handle.await.unwrap();
        cat_gossip_actor_handle.await.unwrap();

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

        // Create topic id.
        let topic_id = [9; 32];

        // Create keypairs.
        let ant_private_key = PrivateKey::new();
        let bat_private_key = PrivateKey::new();
        let cat_private_key = PrivateKey::new();

        let ant_public_key = ant_private_key.public_key();
        let bat_public_key = bat_private_key.public_key();
        let cat_public_key = cat_private_key.public_key();

        // Create endpoints.
        let ant_discovery = StaticProvider::new();
        let ant_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(ant_private_key))
            .add_discovery(ant_discovery.clone())
            .bind()
            .await
            .unwrap();

        let bat_discovery = StaticProvider::new();
        let bat_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(bat_private_key))
            .add_discovery(bat_discovery.clone())
            .bind()
            .await
            .unwrap();

        let cat_discovery = StaticProvider::new();
        let cat_endpoint = IrohEndpoint::builder()
            .secret_key(from_private_key(cat_private_key))
            .add_discovery(cat_discovery.clone())
            .bind()
            .await
            .unwrap();

        // Obtain ant's node information including direct addresses.
        let ant_addrs = ant_endpoint.node_addr();
        let ant_node_info = NodeInfo::new(from_public_key(ant_public_key))
            .with_direct_addresses(ant_addrs.direct_addresses);

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_node_info(ant_node_info);

        // Spawn gossip actors.
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            Actor::spawn(None, Gossip, ant_endpoint.clone())
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            Actor::spawn(None, Gossip, bat_endpoint.clone())
                .await
                .unwrap();
        let (cat_gossip_actor, cat_gossip_actor_handle) =
            Actor::spawn(None, Gossip, cat_endpoint.clone())
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

        let (ant_to_gossip, mut ant_from_gossip) =
            call!(ant_gossip_actor, ToGossip::Subscribe, topic_id, ant_peers).unwrap();
        let (bat_to_gossip, mut bat_from_gossip) =
            call!(bat_gossip_actor, ToGossip::Subscribe, topic_id, bat_peers).unwrap();

        // Send message from ant to bat.
        let ant_msg_to_bat = b"hi bat!".to_vec();
        ant_to_gossip.send(ant_msg_to_bat.clone()).await.unwrap();

        // Ensure bat receives the message from ant.
        let Some(msg) = bat_from_gossip.recv().await else {
            panic!("expected msg from ant")
        };

        assert_eq!(msg, (ant_msg_to_bat, ant_public_key));

        // Send message from bat to ant.
        let bat_msg_to_ant = b"oh hey ant!".to_vec();
        bat_to_gossip.send(bat_msg_to_ant.clone()).await.unwrap();

        // Ensure ant receives the message from bat.
        let Some(msg) = ant_from_gossip.recv().await else {
            panic!("expected msg from bat")
        };

        assert_eq!(msg, (bat_msg_to_ant, bat_public_key));

        // Stop the gossip actor and router for ant (going offline).
        ant_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();
        ant_router.shutdown().await.unwrap();

        // Cat joins the gossip topic (using ant as bootstrap).
        let cat_peers = vec![ant_public_key];

        let (cat_to_gossip, mut cat_from_gossip) =
            call!(cat_gossip_actor, ToGossip::Subscribe, topic_id, cat_peers).unwrap();

        // Send message from cat to bat.
        let cat_msg_to_bat = b"hi bat!".to_vec();
        cat_to_gossip.send(cat_msg_to_bat.clone()).await.unwrap();

        // Briefly sleep to allow processing of sent message.
        sleep(Duration::from_millis(50)).await;

        // Ensure bat has not received the message from cat.
        assert_eq!(bat_from_gossip.try_recv(), Err(TryRecvError::Empty));

        // Send message from bat to cat.
        let bat_msg_to_cat = b"anyone out there?".to_vec();
        bat_to_gossip.send(bat_msg_to_cat.clone()).await.unwrap();

        // Briefly sleep to allow processing of sent message.
        sleep(Duration::from_millis(50)).await;

        // Ensure cat has not received the message from bat.
        assert_eq!(cat_from_gossip.try_recv(), Err(TryRecvError::Empty));

        // At this point we have proof of partition; bat and cat are subscribed to the same gossip
        // topic but cannot "hear" one another.

        // Obtain bat's node information including direct addresses.
        let bat_addrs = bat_endpoint.node_addr();
        let bat_node_info = NodeInfo::new(from_public_key(bat_public_key))
            .with_direct_addresses(bat_addrs.direct_addresses);

        // Cat discovers bat through some out-of-band process.
        cat_discovery.add_node_info(bat_node_info);

        // Cat explicitly joins bat on the gossip topic.
        let _ = cat_gossip_actor.cast(ToGossip::JoinPeers(topic_id, vec![bat_public_key]));

        // Send message from cat to bat.
        let cat_msg_to_bat = b"you there bat?".to_vec();
        cat_to_gossip.send(cat_msg_to_bat.clone()).await.unwrap();

        // Briefly sleep to allow processing of sent message.
        sleep(Duration::from_millis(50)).await;

        // Ensure bat receives the message from cat.
        let Some(msg) = bat_from_gossip.recv().await else {
            panic!("expected msg from cat")
        };

        assert_eq!(msg, (cat_msg_to_bat, cat_public_key));

        // Send message from bat to cat.
        let bat_msg_to_cat = b"yoyo!".to_vec();
        bat_to_gossip.send(bat_msg_to_cat.clone()).await.unwrap();

        // Briefly sleep to allow processing of sent message.
        sleep(Duration::from_millis(500)).await;

        // Ensure cat receives the message from bat.
        let Some(msg) = cat_from_gossip.recv().await else {
            panic!("expected msg from bat")
        };

        assert_eq!(msg, (bat_msg_to_cat, bat_public_key));

        // Stop gossip actors.
        bat_gossip_actor.stop(None);
        bat_gossip_actor_handle.await.unwrap();
        cat_gossip_actor.stop(None);
        cat_gossip_actor_handle.await.unwrap();

        // Shutdown routers.
        bat_router.shutdown().await.unwrap();
        cat_router.shutdown().await.unwrap();
    }
}
