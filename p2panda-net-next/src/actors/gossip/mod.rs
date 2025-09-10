// SPDX-License-Identifier: MIT OR Apache-2.0

//! An `iroh`-specific gossip actor for message broadcast.

mod listener;
mod receiver;
mod sender;
mod session;

use std::collections::HashMap;

use iroh::Endpoint as IrohEndpoint;
use iroh_gossip::net::Gossip as IrohGossip;
use iroh_gossip::proto::{Config as IrohGossipConfig, DeliveryScope as IrohDeliveryScope};
use p2panda_core::PublicKey;
use ractor::{
    Actor, ActorId, ActorProcessingErr, ActorRef, Message, RpcReplyPort, SupervisionEvent,
};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::oneshot::{self, Sender as OneshotSender};
use tracing::{debug, warn};

use crate::actors::gossip::session::GossipSession;
use crate::network::{FromNetwork, ToNetwork};
use crate::{TopicId, from_public_key};

pub enum ToGossip {
    /// Return a handle to the iroh gossip actor.
    ///
    /// This is required when registering the gossip ALPN with the router.
    Handle(RpcReplyPort<IrohGossip>),

    /// Join the given topic, using the given peers as gossip bootstrap nodes.
    Join(
        TopicId,
        Vec<PublicKey>,
        RpcReplyPort<(Sender<ToNetwork>, Receiver<FromNetwork>)>,
    ),

    /// Joined a topic by connecting to the given peers.
    Joined {
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
        session_id: ActorId,
    },
}

impl Message for ToGossip {}

pub struct GossipState {
    gossip: IrohGossip,
    sessions: HashMap<ActorId, TopicId>,
    from_gossip_senders: HashMap<TopicId, Vec<Sender<FromNetwork>>>,
    gossip_joined_senders: HashMap<ActorId, OneshotSender<u8>>,
    topic_delivery_scopes: HashMap<TopicId, Vec<IrohDeliveryScope>>,
}

pub struct Gossip;

impl Actor for Gossip {
    type State = GossipState;
    type Msg = ToGossip;
    type Arguments = (IrohEndpoint, IrohGossipConfig);

    // Configure the gossip actor.
    //
    // A cloned IrohEndpoint is passed in when this actor is spawned by the Endpoint actor.
    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (endpoint, config) = args;

        let gossip = IrohGossip::builder()
            .max_message_size(config.max_message_size)
            .membership_config(config.membership)
            .broadcast_config(config.broadcast)
            .spawn(endpoint);

        let sessions = HashMap::new();
        let from_gossip_senders = HashMap::new();
        let gossip_joined_senders = HashMap::new();
        let topic_delivery_scopes = HashMap::new();

        // TODO: The router needs to be configured to accept gossip protocol.
        // This needs to be done when the router is built.
        // Consider how to do this via config.

        let state = GossipState {
            gossip,
            sessions,
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
            ToGossip::Join(topic_id, peers, reply) => {
                // Channel to receive messages from the user (to the network).
                let (to_network_tx, to_network_rx) = mpsc::channel(128);
                // Channel to receive messages from the network (to the user).
                let (from_network_tx, from_network_rx) = mpsc::channel(128);

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

                // Spawn the session actor with the gossip topic subscription.
                let (gossip_session_actor, _) = Actor::spawn_linked(
                    None,
                    GossipSession::new(myself.clone()),
                    (subscription, to_network_rx, gossip_joined_rx),
                    myself.clone().into(),
                )
                .await?;

                // Associate the session actor with the topic.
                let _ = state
                    .sessions
                    .insert(gossip_session_actor.get_id(), topic_id);

                // Associate the session actor with the gossip joined sender.
                let _ = state
                    .gossip_joined_senders
                    .insert(gossip_session_actor.get_id(), gossip_joined_tx);

                // Associate the user channel (sender) with the topic.
                state
                    .from_gossip_senders
                    .entry(topic_id)
                    .or_default()
                    .push(from_network_tx);

                // Return sender / receiver pair to the user.
                if !reply.is_closed() {
                    // TODO: Handle case where receiver channel has been dropped.
                    let _ = reply.send((to_network_tx, from_network_rx));
                }

                Ok(())
            }
            ToGossip::ReceivedMessage {
                bytes,
                delivered_from,
                delivery_scope,
                session_id,
            } => {
                let msg = FromNetwork::GossipMessage {
                    bytes,
                    delivered_from,
                };

                if let Some(topic_id) = state.sessions.get(&session_id) {
                    // Store the delivery scope of the received message.
                    state
                        .topic_delivery_scopes
                        .entry(*topic_id)
                        .or_default()
                        .push(delivery_scope);

                    // Write the received bytes to all subscribers for the associated topic.
                    if let Some(senders) = state.from_gossip_senders.get(topic_id) {
                        for sender in senders {
                            sender.send(msg.clone()).await?
                        }
                    }
                }

                Ok(())
            }
            // TODO: Handle overlay events.
            //
            // We want to track our neighours for each session (topic).
            //
            // The `Joined` event describes the peers we initially connect with.
            // These are used to populate the set of neighours for the session.
            //
            // `NeighborUp` adds a new peer to the set of neighours.
            //
            // `NeighborDown` removes a peer from the set of neighbours.
            //
            // We need some additional logic to "rejoin" the gossip topic if / when the bootstrap
            // peer(s) (ie. the peers we supplied when calling `Join`) have all transitioned to `NeighborDown`.
            //
            // Open question: Do we kill the current session and spawn an entirely new one? Or
            // would we rather send a `Rejoin` event to the session?
            //
            // Another possibility: The iroh gossip `Sender` exposes a `join_peers()` method. Could
            // we call that each time we discovered a new peer interested in our topic (via ambient
            // discovery)? This might remove the neew to "rejoin".
            ToGossip::Joined { peers, session_id } => {
                // Inform the gossip sender actor that the overlay has been joined.
                if let Some(gossip_joined_tx) = state.gossip_joined_senders.remove(&session_id) {
                    if gossip_joined_tx.send(1).is_err() {
                        warn!("oneshot gossip joined receiver dropped")
                    }
                }

                // TODO: Proper handling.
                if let Some(topic_id) = state.sessions.get(&session_id) {
                    debug!("joined topic {:?} with peers: {:?}", topic_id, peers);
                }

                Ok(())
            }
            ToGossip::NeighborUp {
                peer: _,
                session_id: _,
            } => todo!(),
            ToGossip::NeighborDown {
                peer: _,
                session_id: _,
            } => todo!(),
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
                if let Some(topic_id) = state.sessions.get(&actor_id) {
                    debug!(
                        "gossip actor: received ready from gossip session actor #{} for topic id {:?}",
                        actor_id, topic_id
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if let Some(topic_id) = state.sessions.remove(&actor_id) {
                    debug!(
                        "gossip actor: gossip session #{} over topic id {:?} terminated with reason: {:?}",
                        actor_id, topic_id, reason
                    );

                    // Drop the channel used to send gossip messages to the user.
                    if let Some(from_gossip_tx) = state.from_gossip_senders.remove(&topic_id) {
                        drop(from_gossip_tx)
                    }
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                // NOTE: We do not respawn the session if it fails. Instead, we simply drop the
                // gossip message sender to the user. The user is expected to handle the error on
                // the receiver and resubscribe to the topic if they wish.

                let actor_id = actor.get_id();
                if let Some(topic_id) = state.sessions.remove(&actor_id) {
                    warn!(
                        "gossip_actor: gossip session #{} over topic id {:?} failed with reason: {}",
                        actor_id, topic_id, panic_msg
                    );

                    if let Some(from_gossip_tx) = state.from_gossip_senders.remove(&topic_id) {
                        drop(from_gossip_tx)
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
    use iroh::discovery::NodeInfo;
    use iroh::discovery::static_provider::StaticProvider;
    use iroh::protocol::Router as IrohRouter;
    use iroh::{Endpoint as IrohEndpoint, Watcher as _};
    use iroh_gossip::ALPN as GOSSIP_ALPN;
    use iroh_gossip::proto::Config as IrohGossipConfig;
    use p2panda_core::PrivateKey;
    use ractor::{Actor, call};

    use crate::network::{FromNetwork, ToNetwork};
    use crate::{from_private_key, from_public_key};

    use super::{Gossip, ToGossip};

    #[tokio::test]
    async fn two_peer_gossip() {
        // Ensure gossip messages are broadcast between two peers: ant and bat.
        //
        // Ant acts as the bootstrap node; they enter the gossip topic alone. Bat learns about ant
        // through some other discovery process and then joins the topic.
        //
        // Each peer sends and receives a single message.

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
        let ant_addrs = ant_endpoint.direct_addresses().initialized().await;
        let ant_node_info = NodeInfo::new(from_public_key(ant_public_key))
            .with_direct_addresses(ant_addrs.into_iter().map(|direct| direct.addr).collect());

        // Bat discovers ant through some out-of-band process.
        bat_discovery.add_node_info(ant_node_info);

        // Spawn gossip actors.
        let gossip_config = IrohGossipConfig::default();
        let (ant_gossip_actor, ant_gossip_actor_handle) =
            Actor::spawn(None, Gossip, (ant_endpoint.clone(), gossip_config.clone()))
                .await
                .unwrap();
        let (bat_gossip_actor, bat_gossip_actor_handle) =
            Actor::spawn(None, Gossip, (bat_endpoint.clone(), gossip_config.clone()))
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

        // Join the gossip topic.
        let ant_peers = Vec::new();
        let bat_peers = vec![ant_public_key];

        let (ant_to_gossip, mut ant_from_gossip) =
            call!(ant_gossip_actor, ToGossip::Join, topic_id, ant_peers).unwrap();
        let (bat_to_gossip, mut bat_from_gossip) =
            call!(bat_gossip_actor, ToGossip::Join, topic_id, bat_peers).unwrap();

        // Send message from ant to bat.
        let ant_msg_to_bat = b"hi bat!".to_vec();
        ant_to_gossip
            .send(ToNetwork::Message {
                bytes: ant_msg_to_bat.clone(),
            })
            .await
            .unwrap();

        // Ensure bat receives the message from ant.
        let Some(msg) = bat_from_gossip.recv().await else {
            panic!("expected msg from ant")
        };

        assert_eq!(
            msg,
            FromNetwork::GossipMessage {
                bytes: ant_msg_to_bat,
                delivered_from: ant_public_key
            }
        );

        // Send message from bat to ant.
        let bat_msg_to_ant = b"oh hey ant!".to_vec();
        bat_to_gossip
            .send(ToNetwork::Message {
                bytes: bat_msg_to_ant.clone(),
            })
            .await
            .unwrap();

        // Ensure ant receives the message from bat.
        let Some(msg) = ant_from_gossip.recv().await else {
            panic!("expected msg from bat")
        };

        assert_eq!(
            msg,
            FromNetwork::GossipMessage {
                bytes: bat_msg_to_ant,
                delivered_from: bat_public_key
            }
        );

        // Stop gossip actors.
        ant_gossip_actor.stop(None);
        bat_gossip_actor.stop(None);
        ant_gossip_actor_handle.await.unwrap();
        bat_gossip_actor_handle.await.unwrap();

        // Shutdown routers.
        bat_router.shutdown().await.unwrap();
        ant_router.shutdown().await.unwrap();
    }
}
