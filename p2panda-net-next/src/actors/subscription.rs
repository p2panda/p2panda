// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subscription actor.
//!
//! This actor is responsible for spawning the gossip and sync actors. It also performs supervision
//! of the spawned actors, restarting them in the event of failure.
//!
//! An iroh `Endpoint` is held as part of the internal state of this actor. This allows an
//! `Endpoint` to be passed into the gossip and sync actors in the event that they need to be
//! respawned.
use iroh::Endpoint as IrohEndpoint;
use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use tracing::{debug, warn};

use crate::actors::gossip::{Gossip, ToGossip};
use crate::actors::sync::{Sync, ToSync};

pub enum ToSubscription {}

pub struct SubscriptionState {
    endpoint: IrohEndpoint,
    gossip_actor: ActorRef<ToGossip>,
    gossip_actor_failures: u16,
    sync_actor: ActorRef<ToSync>,
    sync_actor_failures: u16,
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
        // Spawn the gossip actor.
        let (gossip_actor, _) = Actor::spawn_linked(
            Some("gossip".to_string()),
            Gossip {},
            endpoint.clone(),
            myself.clone().into(),
        )
        .await?;

        // Spawn the sync actor.
        let (sync_actor, _) =
            Actor::spawn_linked(Some("sync".to_string()), Sync {}, (), myself.into()).await?;

        let state = SubscriptionState {
            endpoint,
            gossip_actor,
            gossip_actor_failures: 0,
            sync_actor,
            sync_actor_failures: 0,
        };

        Ok(state)
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
                    debug!("subscription actor: received ready from {} actor", name);
                }
            }
            SupervisionEvent::ActorFailed(actor, panic_msg) => {
                match actor.get_name().as_deref() {
                    Some("gossip") => {
                        warn!("subscription actor: gossip actor failed: {}", panic_msg);

                        // Respawn the gossip actor.
                        let (gossip_actor, _) = Actor::spawn_linked(
                            Some("gossip".to_string()),
                            Gossip {},
                            state.endpoint.clone(),
                            myself.clone().into(),
                        )
                        .await?;

                        state.gossip_actor_failures += 1;
                        state.gossip_actor = gossip_actor;
                    }
                    Some("sync") => {
                        warn!("subscription actor: sync actor failed: {}", panic_msg);

                        // Respawn the sync actor.
                        let (sync_actor, _) = Actor::spawn_linked(
                            Some("sync".to_string()),
                            Sync {},
                            (),
                            myself.clone().into(),
                        )
                        .await?;

                        state.sync_actor_failures += 1;
                        state.sync_actor = sync_actor;
                    }
                    _ => (),
                }
            }
            SupervisionEvent::ActorTerminated(actor, _last_state, _reason) => {
                if let Some(name) = actor.get_name() {
                    debug!("subscription actor: {} actor terminated", name);
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
