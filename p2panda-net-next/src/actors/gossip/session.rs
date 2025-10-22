// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gossip session actor.
//!
//! This actor is responsible for supervising a gossip session over a single topic; a separate
//! instance is spawned for each subscribed topic. The actor waits for the topic to be joined
//! and then spawns sender and receiver actors. It receives gossip events from the receiver and
//! forwards them up the chain to the main gossip orchestration actor.
use std::time::Duration;

use iroh::NodeId;
use iroh_gossip::api::{Event as IrohEvent, GossipTopic as IrohGossipTopic};
use p2panda_core::PublicKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tokio::sync::mpsc::Receiver;
use tokio::sync::oneshot::Receiver as OneshotReceiver;
use tracing::{debug, warn};

use crate::TopicId;
use crate::actors::gossip::ToGossip;
use crate::actors::gossip::joiner::{GossipJoiner, ToGossipJoiner};
use crate::actors::gossip::listener::GossipListener;
use crate::actors::gossip::receiver::{GossipReceiver, ToGossipReceiver};
use crate::actors::gossip::sender::{GossipSender, ToGossipSender};
use crate::utils::to_public_key;

pub enum ToGossipSession {
    /// An event received from the gossip overlay.
    ProcessEvent(IrohEvent),

    /// Joined the gossip overlay with the given peers as direct neighbors.
    ProcessJoined(Vec<NodeId>),

    /// Join the given set of peers.
    JoinPeers(Vec<NodeId>),
}

pub struct GossipSessionState {
    topic_id: TopicId,
    gossip_joiner_actor: ActorRef<ToGossipJoiner>,
    gossip_sender_actor: ActorRef<ToGossipSender>,
    gossip_receiver_actor: ActorRef<ToGossipReceiver>,
}

pub struct GossipSession {
    gossip_actor: ActorRef<ToGossip>,
}

impl GossipSession {
    pub fn new(gossip_actor: ActorRef<ToGossip>) -> Self {
        Self { gossip_actor }
    }
}

impl Actor for GossipSession {
    type State = GossipSessionState;

    type Msg = ToGossipSession;

    type Arguments = (
        TopicId,
        IrohGossipTopic,
        Receiver<Vec<u8>>,
        OneshotReceiver<u8>,
    );

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (topic_id, subscription, receiver_from_user, gossip_joined) = args;

        let (sender, receiver) = subscription.split();

        let (gossip_sender_actor, _) = Actor::spawn_linked(
            None,
            GossipSender,
            (sender.clone(), gossip_joined),
            myself.clone().into(),
        )
        .await?;

        // TODO: Consider carefully whether this requires supervision and, if so, what actions to
        // take on termination and failure.
        let (gossip_joiner_actor, _) =
            Actor::spawn_linked(None, GossipJoiner, sender, myself.clone().into()).await?;

        let (gossip_receiver_actor, _) = Actor::spawn_linked(
            None,
            GossipReceiver::new(myself.clone()),
            receiver,
            myself.clone().into(),
        )
        .await?;

        // The channel listener receives messages from userland and forwards them to the gossip
        // sender.
        let (_gossip_listener_actor, _) = Actor::spawn_linked(
            None,
            GossipListener::new(gossip_sender_actor.clone()),
            receiver_from_user,
            myself.clone().into(),
        )
        .await?;

        let state = GossipSessionState {
            topic_id,
            gossip_joiner_actor,
            gossip_sender_actor,
            gossip_receiver_actor,
        };

        Ok(state)
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Gossip events are passed up the chain to the main gossip actor.
        //
        // We perform type conversion here to reduce the workload of the gossip actor.
        match message {
            ToGossipSession::ProcessJoined(peers) => {
                let topic_id = state.topic_id;
                let peers: Vec<PublicKey> = peers.into_iter().map(to_public_key).collect();
                let session_id = myself.get_id();

                let _ = self.gossip_actor.cast(ToGossip::Joined {
                    topic_id,
                    peers,
                    session_id,
                });
            }
            ToGossipSession::ProcessEvent(event) => match event {
                IrohEvent::Lagged => {
                    warn!("gossip session actor: missed messages - dropping gossip event")
                }
                IrohEvent::Received(msg) => {
                    let bytes = msg.content.into();
                    let delivered_from = to_public_key(msg.delivered_from);
                    let delivery_scope = msg.scope;
                    let topic_id = state.topic_id;
                    let session_id = myself.get_id();

                    let _ = self.gossip_actor.cast(ToGossip::ReceivedMessage {
                        bytes,
                        delivered_from,
                        delivery_scope,
                        topic_id,
                        session_id,
                    });
                }
                IrohEvent::NeighborUp(peer) => {
                    let peer = to_public_key(peer);
                    let session_id = myself.get_id();

                    let _ = self
                        .gossip_actor
                        .cast(ToGossip::NeighborUp { peer, session_id });
                }
                IrohEvent::NeighborDown(peer) => {
                    let peer = to_public_key(peer);
                    let session_id = myself.get_id();

                    let _ = self
                        .gossip_actor
                        .cast(ToGossip::NeighborDown { peer, session_id });
                }
            },
            ToGossipSession::JoinPeers(peers) => {
                let _ = state
                    .gossip_joiner_actor
                    .cast(ToGossipJoiner::JoinPeers(peers));
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
                if actor_id == state.gossip_sender_actor.get_id() {
                    debug!(
                        "gossip session actor: received ready from gossip sender actor #{}",
                        actor_id
                    )
                } else if actor_id == state.gossip_receiver_actor.get_id() {
                    debug!(
                        "gossip session actor: received ready from gossip receiver actor #{}",
                        actor_id
                    )
                }
            }
            // We're not interested in respawning a terminated actor in the context of a gossip
            // session. We simply process any queued messages in the remaining actor and stop
            // the session. The main gossip actor will be alerted of the session outcome by a
            // supervision event. The same is true for a failed sender or receiver actor.
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if actor_id == state.gossip_sender_actor.get_id() {
                    debug!(
                        "gossip session actor: gossip sender actor #{} terminated with reason: {:?}",
                        actor_id, reason
                    );
                } else if actor_id == state.gossip_receiver_actor.get_id() {
                    debug!(
                        "gossip session actor: gossip receiver actor #{} terminated with reason: {:?}",
                        actor_id, reason
                    );
                }

                // Process any remaining messages in the queue of the gossip sender and receiver
                // actors, waiting a maximum of 100 milliseconds for their collective exit.
                myself
                    .drain_children_and_wait(Some(Duration::from_millis(100)))
                    .await;
                myself.stop(Some("lost connection to gossip overlay".to_string()));
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                let actor_id = actor.get_id();
                if actor_id == state.gossip_sender_actor.get_id() {
                    debug!(
                        "gossip session actor: gossip sender actor #{} failed with message: {:?}",
                        actor_id, panic_msg
                    );
                } else if actor_id == state.gossip_receiver_actor.get_id() {
                    debug!(
                        "gossip session actor: gossip receiver actor #{} failed with message: {:?}",
                        actor_id, panic_msg
                    );
                }

                myself
                    .drain_children_and_wait(Some(Duration::from_millis(100)))
                    .await;
                myself.stop(Some("lost connection to gossip overlay".to_string()));
            }
            _ => (),
        }

        Ok(())
    }
}
