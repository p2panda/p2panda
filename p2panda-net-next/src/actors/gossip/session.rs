// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gossip session actor.
//!
//! This actor is responsible for supervising a gossip session over a single topic; a separate
//! instance is spawned for each subscribed topic. The actor waits for the topic to be joined
//! and then spawns sender and receiver actors. It receives gossip events from the receiver and
//! forwards them up the chain to the main gossip orchestration actor.

use std::time::Duration;

use iroh_gossip::net::{
    Event as IrohEvent, GossipEvent as IrohGossipEvent, GossipTopic as IrohGossipTopic,
};
use p2panda_core::PublicKey;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use tokio::sync::mpsc::Receiver;
use tracing::{debug, warn};

use crate::actors::gossip::listener::{GossipListener, ToGossipListener};
use crate::actors::gossip::receiver::{GossipReceiver, ToGossipReceiver};
use crate::actors::gossip::sender::{GossipSender, ToGossipSender};
use crate::actors::gossip::ToGossip;
use crate::network::ToNetwork;
use crate::to_public_key;

pub enum ToGossipSession {
    /// An event received from the gossip overlay.
    ProcessEvent(IrohEvent),
}

impl Message for ToGossipSession {}

pub struct GossipSessionState {
    gossip_sender_actor: ActorRef<ToGossipSender>,
    gossip_receiver_actor: ActorRef<ToGossipReceiver>,
    gossip_listener_actor: ActorRef<ToGossipListener>,
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
    // TODO: We also need to receive a channel receiver from userland.
    // That is then passed into the spawned listener.
    type Arguments = (IrohGossipTopic, Receiver<ToNetwork>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (mut subscription, receiver_from_user) = args;

        subscription.joined().await?;

        let (sender, receiver) = subscription.split();

        let (gossip_sender_actor, _) =
            Actor::spawn_linked(None, GossipSender, sender, myself.clone().into()).await?;

        let (gossip_receiver_actor, _) = Actor::spawn_linked(
            None,
            GossipReceiver::new(myself.clone()),
            receiver,
            myself.clone().into(),
        )
        .await?;

        // TODO: Spawn the channel listener.
        // Must take a reference to the `gossip_sender_actor` for direct message passing.
        // The channel listener receives messages from userland and forwards them to the gossip
        // sender.
        let (gossip_listener_actor, _) = Actor::spawn_linked(
            None,
            GossipListener::new(gossip_sender_actor.clone()),
            receiver_from_user,
            myself.clone().into(),
        )
        .await?;

        let state = GossipSessionState {
            gossip_sender_actor,
            gossip_receiver_actor,
            gossip_listener_actor,
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
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToGossipSession::ProcessEvent(event) => match event {
                IrohEvent::Lagged => {
                    warn!("gossip session actor: missed messages - dropping gossip event")
                }
                // Gossip events are passed up the chain to the main gossip actor.
                //
                // We perform type conversion here to reduce the workload of the gossip actor.
                IrohEvent::Gossip(gossip_event) => match gossip_event {
                    IrohGossipEvent::Joined(peers) => {
                        let peers: Vec<PublicKey> = peers.into_iter().map(to_public_key).collect();
                        let session_id = myself.get_id();

                        let _ = self
                            .gossip_actor
                            .cast(ToGossip::Joined { peers, session_id });
                    }
                    IrohGossipEvent::Received(msg) => {
                        let bytes = msg.content.into();
                        let delivered_from = to_public_key(msg.delivered_from);
                        let delivery_scope = msg.scope;
                        let session_id = myself.get_id();

                        let _ = self.gossip_actor.cast(ToGossip::ReceivedMessage {
                            bytes,
                            delivered_from,
                            delivery_scope,
                            session_id,
                        });
                    }
                    IrohGossipEvent::NeighborUp(peer) => {
                        let peer = to_public_key(peer);
                        let session_id = myself.get_id();

                        let _ = self
                            .gossip_actor
                            .cast(ToGossip::NeighborUp { peer, session_id });
                    }
                    IrohGossipEvent::NeighborDown(peer) => {
                        let peer = to_public_key(peer);
                        let session_id = myself.get_id();

                        let _ = self
                            .gossip_actor
                            .cast(ToGossip::NeighborUp { peer, session_id });
                    }
                },
            },
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
