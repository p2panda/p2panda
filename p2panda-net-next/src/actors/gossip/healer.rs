// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subscribe to address book updates and rejoin the gossip with the given nodes if we're actively
//! interested in the associated topics.
use p2panda_discovery::address_book::NodeInfo as _;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, call, registry};
use tokio::sync::broadcast::Receiver;
use tracing::debug;

use crate::TopicId;
use crate::actors::address_book::{ADDRESS_BOOK, ToAddressBook, TopicEvent};
use crate::actors::gossip::session::ToGossipSession;
use crate::actors::{ActorNamespace, with_namespace};
use crate::utils::from_public_key;

pub enum ToGossipHealer {
    /// Subscribe to changes regarding nodes for our topics of interest.
    SubscribeToAddressBook(TopicId),

    /// Wait for an event on the address book subscription channel.
    WaitForEvent,
}

pub struct GossipHealerState {
    actor_namespace: ActorNamespace,
    receiver: Option<Receiver<TopicEvent>>,
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
                if let Some(address_book_actor) =
                    registry::where_is(with_namespace(ADDRESS_BOOK, &state.actor_namespace))
                {
                    let actor: ActorRef<ToAddressBook> = address_book_actor.into();

                    let receiver = call!(actor, ToAddressBook::SubscribeTopicChanges, topic)
                        .expect("address book actor should handle call");
                    state.receiver = Some(receiver);

                    // Invoke the handler to wait for the first event on the receiver.
                    let _ = myself.cast(ToGossipHealer::WaitForEvent);
                } else {
                    panic!("address book actor unavailable")
                };
            }
            ToGossipHealer::WaitForEvent => {
                if let Some(receiver) = &mut state.receiver {
                    match receiver.recv().await {
                        Ok(event) => {
                            debug!("received address book event: {:?}", event);

                            let node_ids = event
                                .node_infos
                                .iter()
                                .map(|node_info| from_public_key(node_info.id()))
                                .collect();

                            // Send the join signal to the gossip session actor.
                            state
                                .gossip_session_ref
                                .send_message(ToGossipSession::JoinPeers(node_ids))?;

                            debug!("sent join peers message to gossip session");

                            // Invoke the handler to wait for the next event on the receiver.
                            let _ = myself.cast(ToGossipHealer::WaitForEvent);
                        }
                        Err(_) => {
                            debug!(
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
