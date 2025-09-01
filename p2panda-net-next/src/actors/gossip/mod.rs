//! An iroh-specific gossip actor for message broadcast.

mod listener;
mod receiver;
mod sender;
mod session;

use std::collections::HashMap;

use iroh::Endpoint as IrohEndpoint;
use iroh_gossip::net::Gossip as IrohGossip;
use iroh_gossip::proto::DeliveryScope as IrohDeliveryScope;
use p2panda_core::PublicKey;
use ractor::{
    Actor, ActorId, ActorProcessingErr, ActorRef, Message, RpcReplyPort, SupervisionEvent,
};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::{debug, warn};

use crate::actors::gossip::session::GossipSession;
use crate::network::{FromNetwork, ToNetwork};
use crate::{from_public_key, TopicId};

// NOTE: The `GossipSession` actor does not know the topic id for it's session. Instead, the
// `Gossip` actor holds a mapping of `topic_id` -> `actor_id` for each gossip subscription
// session.

pub enum ToGossip {
    /// Return a handle to the iroh gossip actor.
    ///
    /// This is required when registering the gossip ALPN with the router.
    Handle(RpcReplyPort<IrohGossip>),

    /// Join the given topic, using the given peers as gossip bootstrap nodes.
    Join {
        topic_id: TopicId,
        peers: Vec<PublicKey>,
        reply: RpcReplyPort<(Sender<ToNetwork>, Receiver<FromNetwork>)>,
    },

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
    // TODO: Store topic_id -> actor_id mappings for session actors.
}

pub struct Gossip;

impl Actor for Gossip {
    type State = GossipState;
    type Msg = ToGossip;
    type Arguments = IrohEndpoint;

    // Configure the Gossip.
    //
    // A cloned IrohEndpoint is passed in when this actor is spawned by the Endpoint actor.
    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        endpoint: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // TODO: Pass config in with the endpoint as arguments.
        // TODO: Configure iroh gossip properly.
        let gossip = IrohGossip::builder().spawn(endpoint.clone()).await?;
        let sessions = HashMap::new();
        let from_gossip_senders = HashMap::new();

        // TODO: The router needs to be configured to accept gossip protocol.
        // This needs to be done when the router is built.
        // Consider how to do this via config.

        let state = GossipState {
            gossip,
            sessions,
            from_gossip_senders,
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
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // TODO: Clean-up on shutdown.
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
            ToGossip::Join {
                topic_id,
                peers,
                reply,
            } => {
                // Channel to receive messages from the user (to the network).
                let (to_network_tx, to_network_rx) = mpsc::channel(128);
                // Channel to receive messages from the network (to the user).
                let (from_network_tx, from_network_rx) = mpsc::channel(128);

                // Convert p2panda public keys to iroh node ids.
                let peers = peers
                    .iter()
                    .map(|key: &PublicKey| from_public_key(*key))
                    .collect();

                let subscription = state.gossip.subscribe(topic_id.into(), peers)?;

                // Spawn the session actor with the gossip topic subscription.
                let (gossip_session_actor, _) = Actor::spawn_linked(
                    None,
                    GossipSession::new(myself.clone()),
                    (subscription, to_network_rx),
                    myself.clone().into(),
                )
                .await?;

                // Associate the session actor with the topic.
                let _ = state
                    .sessions
                    .insert(gossip_session_actor.get_id(), topic_id);

                // Associate the user channel (sender) with the topic.
                let _ = state
                    .from_gossip_senders
                    .entry(topic_id)
                    .or_insert_with(Vec::new)
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
                // TODO: Write the received bytes into the appropriate user channel.
                todo!()
            }
            ToGossip::Joined { peers, session_id } => todo!(),
            ToGossip::NeighborUp { peer, session_id } => todo!(),
            ToGossip::NeighborDown { peer, session_id } => todo!(),
        }
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
                if let Some(topic_id) = state.sessions.get(&actor_id) {
                    debug!(
                        "received ready from gossip session actor #{} for topic id {:?}",
                        actor_id, topic_id
                    );
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if let Some(topic_id) = state.sessions.remove(&actor_id) {
                    debug!(
                        "gossip session #{} over topic id {:?} terminated with reason: {:?}",
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
                        "gossip session #{} over topic id {:?} failed with reason: {}",
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
