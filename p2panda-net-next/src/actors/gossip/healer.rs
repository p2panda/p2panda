// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subscribe to address book updates and rejoin the gossip with the given nodes if we're actively
//! interested in the associated topics.
use std::collections::HashSet;

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tracing::trace;

use crate::actors::ActorNamespace;
use crate::actors::address_book::watch_topic;
use crate::actors::address_book::watchers::WatcherReceiver;
use crate::actors::gossip::session::ToGossipSession;
use crate::utils::from_public_key;
use crate::{NodeId, TopicId};

pub enum ToGossipHealer {
    /// Subscribe to changes regarding nodes for our topics of interest.
    SubscribeToAddressBook(TopicId),

    /// Wait for an event on the address book subscription channel.
    WaitForEvent,
}

pub struct GossipHealerState {
    actor_namespace: ActorNamespace,
    receiver: Option<WatcherReceiver<HashSet<NodeId>>>,
    gossip_session_ref: ActorRef<ToGossipSession>,
}

#[derive(Default)]
pub struct GossipHealer;

impl ThreadLocalActor for GossipHealer {
    type State = GossipHealerState;
    type Msg = ToGossipHealer;
    type Arguments = (ActorNamespace, TopicId, ActorRef<ToGossipSession>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (actor_namespace, topic, gossip_session_ref) = args;

        // Invoke the handler to subscribe to address book events.
        let _ = myself.cast(ToGossipHealer::SubscribeToAddressBook(topic));

        Ok(GossipHealerState {
            actor_namespace,
            receiver: None,
            gossip_session_ref,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        drop(state.receiver.take());
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ToGossipHealer::SubscribeToAddressBook(topic) => {
                let receiver = watch_topic(state.actor_namespace.clone(), topic, true).await?;
                state.receiver = Some(receiver);

                // Invoke the handler to wait for the first event on the receiver.
                let _ = myself.cast(ToGossipHealer::WaitForEvent);
            }
            ToGossipHealer::WaitForEvent => {
                if let Some(receiver) = &mut state.receiver {
                    match receiver.recv().await {
                        Some(event) => {
                            let node_ids =
                                Vec::from_iter(event.value.into_iter().map(from_public_key));

                            // Send the join signal to the gossip session actor.
                            state
                                .gossip_session_ref
                                .send_message(ToGossipSession::JoinPeers(node_ids))?;

                            // Invoke the handler to wait for the next event on the receiver.
                            let _ = myself.cast(ToGossipHealer::WaitForEvent);
                        }
                        None => {
                            trace!(
                                "gossip healer actor: address book dropped broadcast tx - channel closed"
                            );
                            myself.stop(Some("receiver channel closed".to_string()));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
