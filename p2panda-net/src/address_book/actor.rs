// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

// TODO: This will come from `p2panda-store` eventually.
use p2panda_discovery::address_book::{BoxedAddressBookStore, BoxedError, NodeInfo as _};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort};
use tracing::debug;

use crate::address_book::report::ConnectionOutcome;
use crate::address_book::watchers::{WatchedNodeInfo, WatchedNodeTopics, WatchedTopic};
use crate::addrs::{NodeInfo, NodeInfoError, NodeTransportInfo, TransportInfo};
use crate::utils::ShortFormat;
use crate::watchers::{UpdatesOnly, WatcherReceiver, WatcherSet};
use crate::{NodeId, TopicId};

pub enum ToAddressBookActor {
    /// Returns information about a node.
    ///
    /// Returns `None` if no information was found for this node.
    NodeInfo(NodeId, RpcReplyPort<Option<NodeInfo>>),

    /// Returns a list of informations about nodes which are all interested in at least one of the
    /// given topics in this set.
    NodeInfosByTopics(Vec<TopicId>, RpcReplyPort<Vec<NodeInfo>>),

    /// Inserts or updates node information into address book. Use this method if adding node
    /// information from a local configuration, trusted, external source, etc.
    ///
    /// Returns `true` if entry got newly inserted or `false` if existing entry was updated.
    /// Previous entries are simply overwritten. Entries with attached transport information get
    /// checked against authenticity and throw an error otherwise.
    InsertNodeInfo(NodeInfo, RpcReplyPort<Result<bool, NodeInfoError>>),

    /// Inserts or updates attached transport info for a node. Use this method if adding transport
    /// information from an untrusted source.
    ///
    /// Transport information is usually exchanged as part of a discovery protocol and should be
    /// considered untrusted.
    ///
    /// This method checks if the given information is authentic and uses a timestamp to apply a
    /// "last write wins" rule. It retuns `true` if the given entry overwritten the previous one or
    /// `false` if the previous entry is already the latest.
    ///
    /// Local data of the node information stay untouched if they already exist, only the
    /// "transports" aspect gets inserted / updated.
    InsertTransportInfo(
        NodeId,
        TransportInfo,
        RpcReplyPort<Result<bool, NodeInfoError>>,
    ),

    /// Sets the list of "topics" this node is "interested" in.
    ///
    /// Topics are usually shared privately and directly with nodes, this is why implementers
    /// usually want to simply overwrite the previous topic set (_not_ extend it).
    SetTopics(NodeId, HashSet<TopicId>),

    /// Add a topic to set of this node.
    AddTopic(NodeId, TopicId),

    /// Remove topic from set of this node.
    RemoveTopic(NodeId, TopicId),

    /// Removes information for a node. Returns `true` if entry was removed and `false` if it does not
    /// exist.
    RemoveNodeInfo(NodeId, RpcReplyPort<bool>),

    /// Remove all node informations which are older than the given duration (from now). Returns
    /// number of removed entries.
    ///
    /// Applications should frequently clean up "old" information about nodes to remove potentially
    /// "useless" data from the network and not unnecessarily share sensitive information, even
    /// when outdated. This method has a similar function as a TTL (Time-To-Life) record but is
    /// less authoritative.
    ///
    /// Please note that a _local_ timestamp is used to determine the age of the information.
    /// Entries will be removed if they haven't been updated in our _local_ database since the
    /// given duration, _not_ when they have been created by the original author.
    RemoveOlderThan(Duration, RpcReplyPort<usize>),

    /// Subscribes to channel informing us about node info changes for a specific node.
    WatchNodeInfo(
        NodeId,
        UpdatesOnly,
        RpcReplyPort<WatcherReceiver<Option<NodeInfo>>>,
    ),

    /// Subscribes to channel informing us about changes of the set of nodes interested in a topic.
    WatchTopic(
        TopicId,
        UpdatesOnly,
        RpcReplyPort<WatcherReceiver<HashSet<NodeId>>>,
    ),

    /// Subscribes to channel informing us about topic changes for a particular node.
    WatchNodeTopics(
        NodeId,
        UpdatesOnly,
        RpcReplyPort<WatcherReceiver<HashSet<TopicId>>>,
    ),

    /// Report outcomes of incoming or outgoing connections.
    Report(NodeId, ConnectionOutcome),

    /// Returns internal address book store.
    Store(RpcReplyPort<BoxedAddressBookStore<NodeId, NodeInfo>>),
}

pub struct AddressBookState {
    store: BoxedAddressBookStore<NodeId, NodeInfo>,
    node_watchers: WatcherSet<NodeId, WatchedNodeInfo>,
    topic_watchers: WatcherSet<TopicId, WatchedTopic>,
    node_topics_watchers: WatcherSet<NodeId, WatchedNodeTopics>,
}

impl AddressBookState {
    async fn node_infos_by_topics(
        &self,
        topics: Vec<TopicId>,
    ) -> Result<Vec<NodeInfo>, BoxedError> {
        let result = self.store.node_infos_by_topics(&topics).await?;
        Ok(result)
    }

    async fn topics_for_node(&self, node_id: &NodeId) -> Result<HashSet<TopicId>, BoxedError> {
        let topics = self.store.node_topics(node_id).await?;
        Ok(topics)
    }
}

pub type AddressBookActorArgs = (BoxedAddressBookStore<NodeId, NodeInfo>,);

#[derive(Default)]
pub struct AddressBookActor;

impl ThreadLocalActor for AddressBookActor {
    type State = AddressBookState;

    type Msg = ToAddressBookActor;

    type Arguments = AddressBookActorArgs;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (store,) = args;
        Ok(AddressBookState {
            store,
            node_watchers: WatcherSet::new(),
            topic_watchers: WatcherSet::new(),
            node_topics_watchers: WatcherSet::new(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Note that critical storage failures will return an `ActorProcessingErr` and cause this
        // actor to restart when supervised.
        match message {
            ToAddressBookActor::InsertNodeInfo(node_info, reply) => {
                // Check signature of information. Is it authentic?
                if let Err(err) = node_info.verify() {
                    let _ = reply.send(Err(err));
                    return Ok(());
                }

                // Overwrite any previously given information if it existed.
                let result = state.store.insert_node_info(node_info.clone()).await?;

                // Inform subscribers about this update. This will only get notified if it really
                // changed.
                state
                    .node_watchers
                    .update(&node_info.node_id, Some(node_info.clone()));

                let _ = reply.send(Ok(result));
            }
            ToAddressBookActor::InsertTransportInfo(node_id, transport_info, reply) => {
                // Check signature of information. Is it authentic?
                if let Err(err) = transport_info.verify(&node_id) {
                    let _ = reply.send(Err(err));
                    return Ok(());
                }

                // Is there already an existing entry? Only replace it when information is newer
                // (it's a simple "last write wins" CRDT based on a logical timestamp) handled
                // inside of `update_transports`.
                //
                // If a node info already exists, only update the "transports" aspect of it and
                // keep any other "local" configuration, otherwise create a new "default" node
                // info.
                let mut node_info = match state.store.node_info(&node_id).await? {
                    Some(current) => current,
                    None => NodeInfo::new(node_id),
                };

                match node_info.update_transports(transport_info) {
                    Ok(is_newer) => {
                        state.store.insert_node_info(node_info.clone()).await?;
                        let _ = reply.send(Ok(is_newer));
                    }
                    Err(err) => {
                        let _ = reply.send(Err(err));
                    }
                }

                // Inform subscribers about this update. This will only get notified if it really
                // changed.
                state
                    .node_watchers
                    .update(&node_info.node_id, Some(node_info.clone()));
            }
            ToAddressBookActor::WatchNodeInfo(node_id, updates_only, reply) => {
                let node_info = state.store.node_info(&node_id).await?;
                let rx = state.node_watchers.subscribe(
                    node_id,
                    updates_only,
                    WatchedNodeInfo::from_node_info(node_info),
                );
                let _ = reply.send(rx);
            }
            ToAddressBookActor::WatchTopic(topic, updates_only, reply) => {
                // Since we don't know where this topic belongs to we need to check both stream
                // types.
                let node_ids: HashSet<NodeId> = state
                    .node_infos_by_topics(vec![topic])
                    .await?
                    .iter()
                    .map(|info| info.id())
                    .collect();

                let rx = state.topic_watchers.subscribe(
                    topic,
                    updates_only,
                    WatchedTopic::from_node_ids(topic, node_ids),
                );
                let _ = reply.send(rx);
            }
            ToAddressBookActor::WatchNodeTopics(node_id, updates_only, reply) => {
                let topics = state.topics_for_node(&node_id).await?;
                let rx = state.node_topics_watchers.subscribe(
                    node_id,
                    updates_only,
                    WatchedNodeTopics::from_topics(node_id, topics),
                );
                let _ = reply.send(rx);
            }
            ToAddressBookActor::Report(remote_node_id, outcome) => {
                let Some(mut node_info) = state.store.node_info(&remote_node_id).await? else {
                    return Ok(());
                };

                let before = node_info.is_stale();

                match outcome {
                    ConnectionOutcome::Successful => {
                        node_info.metrics.report_successful_connection();
                    }
                    ConnectionOutcome::Failed => {
                        node_info.metrics.report_failed_connection();
                    }
                }

                let after = node_info.is_stale();

                match (before, after) {
                    (true, false) => {
                        debug!(
                            remote_node_id = %remote_node_id.fmt_short(),
                            "mark node as active after being stale"
                        );
                    }
                    (false, true) => {
                        debug!(remote_node_id = %remote_node_id.fmt_short(), "mark node as stale");
                    }
                    _ => (),
                }

                state.store.insert_node_info(node_info).await?;
            }
            ToAddressBookActor::NodeInfo(node_id, reply) => {
                let result = state.store.node_info(&node_id).await?;
                let _ = reply.send(result);
            }
            ToAddressBookActor::NodeInfosByTopics(topics, reply) => {
                let result = state.node_infos_by_topics(topics).await?;
                let _ = reply.send(result);
            }
            ToAddressBookActor::SetTopics(node_id, topics) => {
                state.store.set_topics(node_id, topics.clone()).await?;

                // Inform subscribers about potential change in set of interested nodes.
                for topic in &topics {
                    let node_ids = state
                        .node_infos_by_topics(vec![*topic])
                        .await?
                        .into_iter()
                        .map(|info| info.id());
                    state
                        .topic_watchers
                        .update(topic, HashSet::from_iter(node_ids));
                }

                // Inform subscribers about changes in set of topics.
                let topics = state.topics_for_node(&node_id).await?;
                state.node_topics_watchers.update(&node_id, topics);
            }
            ToAddressBookActor::AddTopic(node_id, topic) => {
                let mut topics = state.store.node_topics(&node_id).await?;
                if topics.insert(topic) {
                    myself.send_message(ToAddressBookActor::SetTopics(node_id, topics))?;
                }
            }
            ToAddressBookActor::RemoveTopic(node_id, topic) => {
                let mut topics = state.store.node_topics(&node_id).await?;
                if topics.remove(&topic) {
                    myself.send_message(ToAddressBookActor::SetTopics(node_id, topics))?;
                }
            }
            ToAddressBookActor::RemoveNodeInfo(node_id, reply) => {
                let result = state.store.remove_node_info(&node_id).await?;
                let _ = reply.send(result);
            }
            ToAddressBookActor::RemoveOlderThan(duration, reply) => {
                let result = state.store.remove_older_than(duration).await?;
                let _ = reply.send(result);
            }
            ToAddressBookActor::Store(reply) => {
                let _ = reply.send(state.store.clone_box());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;
    use p2panda_discovery::address_book::NodeInfo as _;
    use ractor::call;
    use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

    use crate::addrs::{
        NodeInfo, NodeMetrics, NodeTransportInfo, TransportAddress, UnsignedTransportInfo,
    };
    use crate::test_utils::test_args;

    use super::{AddressBookActor, ToAddressBookActor};

    #[tokio::test]
    async fn insert_node_and_transport_info() {
        let (args, store) = test_args();

        let spawner = ThreadLocalActorSpawner::new();

        let (actor, _handle) = AddressBookActor::spawn(None, (Box::new(store),), spawner)
            .await
            .unwrap();

        // Insert new node info.
        let node_info = NodeInfo::new(args.public_key.clone());
        let result = call!(actor, ToAddressBookActor::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Overwriting node info should return "false".
        let mut node_info = NodeInfo::new(args.public_key.clone());
        node_info.bootstrap = true;
        let result = call!(actor, ToAddressBookActor::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Bootstrap should be set to "true", as node info was still overwritten.
        let result = call!(actor, ToAddressBookActor::NodeInfo, args.public_key.clone())
            .unwrap()
            .expect("node info exists in store");
        assert!(result.bootstrap);
        assert!(result.transports().is_none());

        // Inserting invalid node info should fail.
        let node_info = {
            NodeInfo {
                node_id: args.public_key.clone(),
                bootstrap: false,
                transports: Some({
                    let mut unsigned = UnsignedTransportInfo::new();
                    unsigned.add_addr(TransportAddress::from_iroh(
                        args.public_key.clone(),
                        Some("https://my.relay.net".parse().unwrap()),
                        [],
                    ));
                    let mut transport_info = unsigned.sign(&args.private_key.clone()).unwrap();
                    transport_info.timestamp = 1234.into(); // Manipulate timestamp to make signature invalid
                    transport_info.into()
                }),
                metrics: NodeMetrics::default(),
            }
        };
        assert!(node_info.verify().is_err());
        let result = call!(actor, ToAddressBookActor::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_err());

        // Inserting transport info should not overwrite "local" data.
        let mut node_info = NodeInfo::new(args.public_key.clone());
        node_info.bootstrap = true;
        let result = call!(actor, ToAddressBookActor::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_ok());

        let transport_info = {
            let mut unsigned = UnsignedTransportInfo::new();
            unsigned.add_addr(TransportAddress::from_iroh(
                args.public_key.clone(),
                Some("https://my.relay.net".parse().unwrap()),
                [],
            ));
            unsigned.sign(&args.private_key).unwrap()
        };
        let result = call!(
            actor,
            ToAddressBookActor::InsertTransportInfo,
            args.public_key.clone(),
            transport_info.into()
        )
        .unwrap();
        assert!(result.is_ok());

        // Even after insertion of new transport info, the "local" bootstrap config is still true.
        let result = call!(actor, ToAddressBookActor::NodeInfo, args.public_key.clone())
            .unwrap()
            .expect("node info exists in store");
        assert!(result.bootstrap);

        // Transport info was set.
        assert!(result.transports().is_some());

        // Inserting invalid transport info should fail.
        let transport_info = {
            let mut unsigned = UnsignedTransportInfo::new();
            unsigned.add_addr(TransportAddress::from_iroh(
                args.public_key.clone(),
                Some("https://my.relay.net".parse().unwrap()),
                [],
            ));
            let mut transport_info = unsigned.sign(&args.private_key.clone()).unwrap();
            transport_info.timestamp = 1234.into(); // Manipulate timestamp to make signature invalid
            transport_info
        };
        assert!(transport_info.verify(&args.public_key).is_err());
        let result = call!(
            actor,
            ToAddressBookActor::InsertTransportInfo,
            args.public_key.clone(),
            transport_info.into()
        )
        .unwrap();
        assert!(result.is_err());

        // Inserting new transport info just creates a "default" object.
        let private_key = PrivateKey::new();
        let public_key = private_key.public_key();
        let transport_info = {
            let mut unsigned = UnsignedTransportInfo::new();
            unsigned.add_addr(TransportAddress::from_iroh(
                public_key,
                Some("https://my.relay.net".parse().unwrap()),
                [],
            ));
            unsigned.sign(&private_key).unwrap()
        };
        let result = call!(
            actor,
            ToAddressBookActor::InsertTransportInfo,
            public_key,
            transport_info.into()
        )
        .unwrap();
        assert!(result.is_ok());

        let result = call!(actor, ToAddressBookActor::NodeInfo, public_key)
            .unwrap()
            .expect("node info exists in store");
        assert!(!result.bootstrap);
        assert!(result.transports().is_some());
    }
}
