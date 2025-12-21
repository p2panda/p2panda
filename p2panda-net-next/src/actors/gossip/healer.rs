// SPDX-License-Identifier: MIT OR Apache-2.0

//! Subscribe to address book updates and rejoin the gossip with the given nodes if we're actively
//! interested in the associated topics.
use std::collections::HashSet;

use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tracing::trace;

use crate::actors::address_book::{watch_node_info, watch_topic};
use crate::actors::gossip::session::ToGossipSession;
use crate::actors::{ActorNamespace, generate_actor_namespace};
use crate::addrs::NodeInfo;
use crate::args::ApplicationArguments;
use crate::utils::from_public_key;
use crate::watchers::WatcherReceiver;
use crate::{NodeId, TopicId};

pub enum ToGossipHealer {
    /// Subscribe to changes regarding nodes for our topics of interest.
    SubscribeToAddressBook(TopicId),

    /// Wait for an event on the address book subscription channel.
    WaitForEvent,
}

pub struct GossipHealerState {
    actor_namespace: ActorNamespace,
    args: ApplicationArguments,
    topic_endpoint_ids: Vec<iroh::EndpointId>,
    topic_watcher: Option<WatcherReceiver<HashSet<NodeId>>>,
    node_watcher: Option<WatcherReceiver<Option<NodeInfo>>>,
    gossip_session_ref: ActorRef<ToGossipSession>,
}

#[derive(Default)]
pub struct GossipHealer;

impl ThreadLocalActor for GossipHealer {
    type State = GossipHealerState;

    type Msg = ToGossipHealer;

    type Arguments = (ApplicationArguments, TopicId, ActorRef<ToGossipSession>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, topic, gossip_session_ref) = args;
        let actor_namespace = generate_actor_namespace(&args.public_key);

        // Invoke the handler to subscribe to address book events.
        let _ = myself.cast(ToGossipHealer::SubscribeToAddressBook(topic));

        Ok(GossipHealerState {
            actor_namespace,
            args,
            topic_endpoint_ids: Vec::new(),
            topic_watcher: None,
            node_watcher: None,
            gossip_session_ref,
        })
    }

    async fn post_stop(
        &self,
        _myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        state.topic_watcher.take();
        state.node_watcher.take();
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
                // Watch for changes in the set of interested nodes for this topic.
                let topic_watcher =
                    watch_topic(state.actor_namespace.clone(), topic, false).await?;
                state.topic_watcher = Some(topic_watcher);

                // Watch for changes of our own transport info to react to connectivity changes.
                let node_watcher =
                    watch_node_info(state.actor_namespace.clone(), state.args.public_key, true)
                        .await?;
                state.node_watcher = Some(node_watcher);

                let _ = myself.cast(ToGossipHealer::WaitForEvent);
            }
            ToGossipHealer::WaitForEvent => {
                let topic_watcher = state
                    .topic_watcher
                    .as_mut()
                    .expect("was initialised before");
                let node_watcher = state.node_watcher.as_mut().expect("was initialised before");

                tokio::select! {
                    Some(event) = topic_watcher.recv() => {
                        // Re-join the gossip overlay when the set of interested nodes changed.
                        //
                        // We receive this set from the address book / discovery layer and utilise
                        // this information coming from an external source to "heal" potential
                        // network fragmentations caused by nodes going offline.
                        //
                        // HyParView can't automatically recover from these fragmentations, this
                        // approach makes it possible & gossipping more robust.
                        state.topic_endpoint_ids = Vec::from_iter(event.value.into_iter().map(from_public_key));
                        state
                            .gossip_session_ref
                            .send_message(ToGossipSession::JoinPeers(state.topic_endpoint_ids.clone()))?;
                    },
                    Some(_) = node_watcher.recv() => {
                        // Re-join the gossip overlay when we've changed our transport info.
                        //
                        // This accommodates for scenarios where our node went offline / into a
                        // degraded connectivity state and then back online again.
                        if !state.topic_endpoint_ids.is_empty() {
                            state
                                .gossip_session_ref
                                .send_message(ToGossipSession::JoinPeers(state.topic_endpoint_ids.clone()))?;
                        }
                    },
                    else => {
                        trace!(
                            "gossip healer actor: address book dropped broadcast tx - channel closed"
                        );
                        myself.stop(Some("topic_watcher channel closed".to_string()));
                    }
                }

                let _ = myself.cast(ToGossipHealer::WaitForEvent);
            }
        }

        Ok(())
    }
}
