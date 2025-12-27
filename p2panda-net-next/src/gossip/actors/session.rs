// SPDX-License-Identifier: MIT OR Apache-2.0

//! Gossip session actor.
//!
//! This actor is responsible for supervising a gossip session over a single topic; a separate
//! instance is spawned for each subscribed topic. The actor waits for the topic to be joined and
//! then spawns sender and receiver actors. It receives gossip events from the receiver and
//! forwards them up the chain to the main gossip orchestration actor.
use std::time::Duration;

use iroh_gossip::api::{Event as IrohEvent, GossipTopic as IrohGossipTopic};
use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};
use ractor::{ActorProcessingErr, ActorRef, SupervisionEvent};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, trace, warn};

use crate::address_book::AddressBook;
use crate::gossip::actors::ToGossipManager;
use crate::gossip::actors::healer::{GossipHealer, ToGossipHealer};
use crate::gossip::actors::joiner::{GossipJoiner, ToGossipJoiner};
use crate::gossip::actors::listener::GossipListener;
use crate::gossip::actors::receiver::{GossipReceiver, ToGossipReceiver};
use crate::gossip::actors::sender::{GossipSender, ToGossipSender};
use crate::iroh_endpoint::to_public_key;
use crate::utils::ShortFormat;
use crate::{NodeId, TopicId};

pub enum ToGossipSession {
    /// An event received from the gossip overlay.
    ProcessEvent(IrohEvent),

    /// Joined the gossip overlay with the given nodes as direct neighbors.
    ProcessJoined(Vec<iroh::EndpointId>),

    /// Join the given set of nodes.
    JoinNodes(Vec<iroh::EndpointId>),
}

pub struct GossipSessionState {
    topic: TopicId,
    #[allow(unused)]
    gossip_healer_actor: ActorRef<ToGossipHealer>,
    gossip_joiner_actor: ActorRef<ToGossipJoiner>,
    gossip_sender_actor: ActorRef<ToGossipSender>,
    gossip_receiver_actor: ActorRef<ToGossipReceiver>,
    gossip_actor: ActorRef<ToGossipManager>,
}

#[derive(Default)]
pub struct GossipSession;

impl ThreadLocalActor for GossipSession {
    type State = GossipSessionState;

    type Msg = ToGossipSession;

    type Arguments = (
        NodeId,
        AddressBook,
        TopicId,
        IrohGossipTopic,
        mpsc::Receiver<Vec<u8>>,
        oneshot::Receiver<()>,
        ActorRef<ToGossipManager>,
        ThreadLocalActorSpawner,
    );

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (
            my_node_id,
            address_book,
            topic,
            subscription,
            receiver_from_user,
            gossip_joined,
            gossip_actor,
            gossip_thread_pool,
        ) = args;

        let (sender, receiver) = subscription.split();

        let (gossip_receiver_actor, _) = GossipReceiver::spawn_linked(
            None,
            (receiver, myself.clone()),
            myself.clone().into(),
            gossip_thread_pool.clone(),
        )
        .await?;

        let (gossip_sender_actor, _) = GossipSender::spawn_linked(
            None,
            (sender.clone(), gossip_joined),
            myself.clone().into(),
            gossip_thread_pool.clone(),
        )
        .await?;

        // The channel listener receives messages from userland and forwards them to the gossip
        // sender.
        let (_gossip_listener_actor, _) = GossipListener::spawn_linked(
            None,
            (receiver_from_user, gossip_sender_actor.clone()),
            myself.clone().into(),
            gossip_thread_pool.clone(),
        )
        .await?;

        let (gossip_joiner_actor, _) = GossipJoiner::spawn_linked(
            None,
            sender,
            myself.clone().into(),
            gossip_thread_pool.clone(),
        )
        .await?;

        let (gossip_healer_actor, _) = GossipHealer::spawn_linked(
            None,
            (my_node_id, address_book, topic, myself.clone()),
            myself.clone().into(),
            gossip_thread_pool.clone(),
        )
        .await?;

        let state = GossipSessionState {
            topic,
            gossip_healer_actor,
            gossip_joiner_actor,
            gossip_sender_actor,
            gossip_receiver_actor,
            gossip_actor,
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
            ToGossipSession::ProcessJoined(nodes) => {
                let topic = state.topic;
                let nodes: Vec<NodeId> = nodes.into_iter().map(to_public_key).collect();
                let session_id = myself.get_id();

                let _ = state.gossip_actor.cast(ToGossipManager::Joined {
                    topic,
                    nodes,
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
                    let topic = state.topic;
                    let session_id = myself.get_id();

                    let _ = state.gossip_actor.cast(ToGossipManager::ReceivedMessage {
                        bytes,
                        delivered_from,
                        delivery_scope,
                        topic,
                        session_id,
                    });
                }
                IrohEvent::NeighborUp(peer) => {
                    let node_id = to_public_key(peer);
                    let session_id = myself.get_id();

                    let _ = state.gossip_actor.cast(ToGossipManager::NeighborUp {
                        node_id,
                        session_id,
                    });
                }
                IrohEvent::NeighborDown(peer) => {
                    let node_id = to_public_key(peer);
                    let session_id = myself.get_id();

                    let _ = state.gossip_actor.cast(ToGossipManager::NeighborDown {
                        node_id,
                        session_id,
                    });
                }
            },
            ToGossipSession::JoinNodes(nodes) => {
                debug!(
                    topic = %state.topic.fmt_short(),
                    nodes = %nodes.fmt_short(),
                    "(re-) join gossip overlay"
                );

                let _ = state
                    .gossip_joiner_actor
                    .cast(ToGossipJoiner::JoinNodes(nodes));
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
                    trace!(%actor_id, "received ready from gossip sender",)
                } else if actor_id == state.gossip_receiver_actor.get_id() {
                    trace!(%actor_id, "received ready from gossip receiver",)
                }
            }
            // We're not interested in respawning a terminated actor in the context of a gossip
            // session. We simply process any queued messages in the remaining actor and stop the
            // session. The main gossip actor will be alerted of the session outcome by a
            // supervision event. The same is true for a failed sender or receiver actor.
            SupervisionEvent::ActorTerminated(actor, _last_state, reason) => {
                let actor_id = actor.get_id();
                if actor_id == state.gossip_sender_actor.get_id() {
                    debug!(%actor_id, "gossip sender actor terminated: {reason:?}",);
                } else if actor_id == state.gossip_receiver_actor.get_id() {
                    debug!(%actor_id, "gossip receiver actor terminated: {reason:?}",);
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
                    warn!(%actor_id, "gossip sender actor failed: {panic_msg:#?}",);
                } else if actor_id == state.gossip_receiver_actor.get_id() {
                    warn!(%actor_id, "gossip receiver actor failed: {panic_msg:#?}",);
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
