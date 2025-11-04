// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subscription actor.
//!
//! This actor is responsible for spawning the gossip and sync actors. It also performs supervision
//! of the spawned actors, restarting them in the event of failure.
//!
//! An iroh `Endpoint` is held as part of the internal state of this actor. This allows an
//! `Endpoint` to be passed into the gossip and sync actors in the event that they need to be
//! respawned.
use std::collections::HashMap;

use iroh::Endpoint as IrohEndpoint;
use ractor::{
    Actor, ActorProcessingErr, ActorRef, Message, RpcReplyPort, SupervisionEvent, call, cast,
};
use tokio::sync::broadcast::Sender as BroadcastSender;
use tokio::sync::mpsc::Sender;
use tracing::{debug, warn};

use crate::actors::gossip::{Gossip, ToGossip};
use crate::actors::sync::{Sync, ToSync};
use crate::actors::{generate_actor_namespace, with_namespace, without_namespace};
use crate::network::{FromNetwork, ToNetwork};
use crate::topic_streams::{EphemeralStream, EphemeralStreamSubscription};
use crate::{TopicId, to_public_key};

pub enum ToSubscription {
    /// Subscribe to the topic ID and return a publishing handle.
    CreateEphemeralStream(TopicId, RpcReplyPort<EphemeralStream>),

    /// Return a subscription handle for the given topic ID.
    EphemeralSubscription(TopicId, RpcReplyPort<EphemeralStreamSubscription>),

    /// Unsubscribe from an ephemeral stream for the given topic ID.
    UnsubscribeEphemeral(TopicId),
}

impl Message for ToSubscription {}

pub struct SubscriptionState {
    endpoint: IrohEndpoint,
    gossip_actor: ActorRef<ToGossip>,
    gossip_actor_failures: u16,
    sync_actor: ActorRef<ToSync>,
    sync_actor_failures: u16,
    to_gossip_senders: HashMap<TopicId, Sender<ToNetwork>>,
    from_gossip_senders: HashMap<TopicId, BroadcastSender<FromNetwork>>,
}

pub struct Subscription;

impl Actor for Subscription {
    type State = SubscriptionState;
    type Msg = ToSubscription;
    type Arguments = IrohEndpoint;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        endpoint: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let actor_namespace = generate_actor_namespace(&to_public_key(endpoint.node_id()));

        // Spawn the gossip actor.
        let (gossip_actor, _) = Actor::spawn_linked(
            Some(with_namespace("gossip", &actor_namespace)),
            Gossip {},
            endpoint.clone(),
            myself.clone().into(),
        )
        .await?;

        // Spawn the sync actor.
        let (sync_actor, _) = Actor::spawn_linked(
            Some(with_namespace("sync", &actor_namespace)),
            Sync {},
            (),
            myself.into(),
        )
        .await?;

        let to_gossip_senders = HashMap::new();
        let from_gossip_senders = HashMap::new();

        let state = SubscriptionState {
            endpoint,
            gossip_actor,
            gossip_actor_failures: 0,
            sync_actor,
            sync_actor_failures: 0,
            to_gossip_senders,
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
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToSubscription::CreateEphemeralStream(topic_id, reply) => {
                let actor_namespace =
                    generate_actor_namespace(&to_public_key(state.endpoint.node_id()));

                // TODO: Ask address book for all peers interested in this topic id.
                let peers = Vec::new();

                // Check if we're already subscribed.
                let stream = if let Some(to_gossip_tx) = state.to_gossip_senders.get(&topic_id) {
                    // Inform the gossip actor about the latest set of peers for this topic id.
                    cast!(state.gossip_actor, ToGossip::JoinPeers(topic_id, peers))?;

                    EphemeralStream::new(topic_id, to_gossip_tx.clone(), actor_namespace)
                } else {
                    // Register a new session with the gossip actor.
                    let (to_gossip_tx, from_gossip_tx) =
                        call!(state.gossip_actor, ToGossip::Subscribe, topic_id, peers)?;

                    // Store the gossip sender which is used to publish messages into the
                    // topic.
                    state
                        .to_gossip_senders
                        .insert(topic_id, to_gossip_tx.clone());

                    // Store the gossip sender. This can be used to create a broadcast receiver
                    // when the user calls `.subscribe()` on `EphemeralStream`.
                    state.from_gossip_senders.insert(topic_id, from_gossip_tx);

                    EphemeralStream::new(topic_id, to_gossip_tx, actor_namespace)
                };

                if !reply.is_closed() {
                    let _ = reply.send(stream);
                }
            }
            ToSubscription::EphemeralSubscription(topic_id, reply) => {
                if let Some(from_gossip_tx) = state.from_gossip_senders.get(&topic_id) {
                    let from_gossip_rx = from_gossip_tx.subscribe();

                    let subscription = EphemeralStreamSubscription::new(topic_id, from_gossip_rx);

                    if !reply.is_closed() {
                        let _ = reply.send(subscription);
                    }
                }
            }
            ToSubscription::UnsubscribeEphemeral(topic_id) => {
                // Drop all senders associated with the topic id..
                let _ = state.to_gossip_senders.remove(&topic_id);
                let _ = state.from_gossip_senders.remove(&topic_id);

                // Tell the gossip actor to unsubscribe from this topic id.
                cast!(state.gossip_actor, ToGossip::Unsubscribe(topic_id))?;
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
                if let Some(name) = actor.get_name() {
                    debug!(
                        "subscription actor: received ready from {} actor",
                        without_namespace(&name)
                    );
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                let actor_namespace =
                    generate_actor_namespace(&to_public_key(state.endpoint.node_id()));

                if let Some(name) = actor.get_name().as_deref() {
                    if name == with_namespace("gossip", &actor_namespace) {
                        warn!("subscription actor: gossip actor failed: {}", panic_msg);

                        // Respawn the gossip actor.
                        let (gossip_actor, _) = Actor::spawn_linked(
                            Some(with_namespace("gossip", &actor_namespace)),
                            Gossip {},
                            state.endpoint.clone(),
                            myself.clone().into(),
                        )
                        .await?;

                        state.gossip_actor_failures += 1;
                        state.gossip_actor = gossip_actor;
                    } else if name == with_namespace("sync", &actor_namespace) {
                        warn!("subscription actor: sync actor failed: {}", panic_msg);

                        // Respawn the sync actor.
                        let (sync_actor, _) = Actor::spawn_linked(
                            Some(with_namespace("sync", &actor_namespace)),
                            Sync {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.sync_actor_failures += 1;
                        state.sync_actor = sync_actor;
                    }
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!(
                        "subscription actor: {} actor terminated",
                        without_namespace(&name)
                    );
                }
            }
            _ => (),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use iroh::Endpoint as IrohEndpoint;
    use ractor::Actor;
    use serial_test::serial;
    use tokio::time::{Duration, sleep};
    use tracing_test::traced_test;

    use super::Subscription;

    #[tokio::test]
    #[traced_test]
    #[serial]
    async fn subscription_child_actors_are_started() {
        let endpoint = IrohEndpoint::builder().bind().await.unwrap();

        let (subscription_actor, subscription_actor_handle) =
            Actor::spawn(Some("subscription".to_string()), Subscription {}, endpoint)
                .await
                .unwrap();

        // Sleep briefly to allow time for all actors to be ready.
        sleep(Duration::from_millis(50)).await;

        subscription_actor.stop(None);
        subscription_actor_handle.await.unwrap();

        assert!(logs_contain(
            "subscription actor: received ready from gossip actor"
        ));
        assert!(logs_contain(
            "subscription actor: received ready from sync actor"
        ));

        assert!(!logs_contain("actor failed"));
    }
}
