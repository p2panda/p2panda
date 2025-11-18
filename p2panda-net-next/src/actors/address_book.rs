// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::error::Error as StdError;
use std::marker::PhantomData;
use std::time::Duration;

// @TODO: This will come from `p2panda-store` eventually.
use p2panda_discovery::address_book::{AddressBookStore, NodeInfo as _};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort};
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::info;

use crate::args::ApplicationArguments;
use crate::{NodeId, NodeInfo, TopicId, TransportInfo};

/// Address book actor name.
pub const ADDRESS_BOOK: &str = "net.address_book";

pub enum ToAddressBook {
    /// Returns information about a node.
    ///
    /// Returns `None` if no information was found for this node.
    NodeInfo(NodeId, RpcReplyPort<Option<NodeInfo>>),

    /// Returns a list of all known node informations.
    #[allow(unused)]
    AllNodeInfos(RpcReplyPort<Vec<NodeInfo>>),

    /// Returns a list of node informations for a selected set.
    #[allow(unused)]
    SelectedNodeInfos(Vec<NodeId>, RpcReplyPort<Vec<NodeInfo>>),

    /// Returns a list of informations about nodes which are all interested in at least one of the
    /// given topics in this set.
    #[allow(unused)]
    NodeInfosBySyncTopics(Vec<TopicId>, RpcReplyPort<Vec<NodeInfo>>),

    /// Returns a list of informations about nodes which are all interested in at least one of the
    /// given topics in this set.
    NodeInfosByEphemeralMessagingTopics(Vec<TopicId>, RpcReplyPort<Vec<NodeInfo>>),

    /// Returns information from a randomly picked node or `None` when no information exists in the
    /// database.
    #[allow(unused)]
    RandomNodeInfo(RpcReplyPort<Option<NodeInfo>>),

    /// Inserts or updates node information into address book. Use this method if adding node
    /// information from a local configuration, trusted, external source, etc.
    ///
    /// Returns `true` if entry got newly inserted or `false` if existing entry was updated.
    /// Previous entries are simply overwritten. Entries with attached transport information get
    /// checked against authenticity and throw an error otherwise.
    InsertNodeInfo(NodeInfo, RpcReplyPort<Result<bool, AddressBookError>>),

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
        RpcReplyPort<Result<bool, AddressBookError>>,
    ),

    /// Sets the list of "topics" for eventually consistent sync this node is "interested" in.
    ///
    /// Topics are usually shared privately and directly with nodes, this is why implementers
    /// usually want to simply overwrite the previous topic set (_not_ extend it).
    SetSyncTopics(NodeId, HashSet<TopicId>),

    /// Sets the list of "topics" for ephemeral messaging this node is "interested" in.
    ///
    /// Topics for gossip overlays (used for ephemeral messaging) are usually shared privately and
    /// directly with nodes, this is why implementers usually want to simply overwrite the previous
    /// topic set (_not_ extend it).
    SetEphemeralMessagingTopics(NodeId, HashSet<TopicId>),

    /// Removes information for a node. Returns `true` if entry was removed and `false` if it does not
    /// exist.
    #[allow(unused)]
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
    #[allow(unused)]
    RemoveOlderThan(Duration, RpcReplyPort<usize>),

    /// Subscribes to channel informing us about changes on node infos for a specific node.
    SubscribeNodeChanges(NodeId, RpcReplyPort<broadcast::Receiver<NodeEvent>>),

    /// Subscribes to channel informing us about changes of the set of nodes interested in a topic
    /// id for eventually consistent and ephemeral streams.
    SubscribeTopicChanges(TopicId, RpcReplyPort<broadcast::Receiver<TopicEvent>>),
}

pub struct AddressBookState<S> {
    args: ApplicationArguments,
    store: S,
    node_subscribers: HashMap<NodeId, broadcast::Sender<NodeEvent>>,
    topic_subscribers: HashMap<TopicId, broadcast::Sender<TopicEvent>>,
}

impl<S> AddressBookState<S>
where
    S: AddressBookStore<NodeId, NodeInfo>,
{
    /// Inform all subscribers about a node info change;
    fn call_node_subscribers(&mut self, node_id: NodeId, node_info: &NodeInfo) {
        let Some(tx) = self.node_subscribers.get(&node_id) else {
            return;
        };

        if tx
            .send(NodeEvent {
                node_id,
                node_info: node_info.clone(),
            })
            .is_err()
        {
            // On an error we know that all receivers have been dropped, so we can remove this
            // subscription as well and clean up after ourselves.
            self.node_subscribers.remove(&node_id);
        }
    }

    async fn call_sync_topic_subscribers(&mut self, topic: TopicId) {
        let Some(tx) = self.topic_subscribers.get(&topic) else {
            return;
        };

        let Ok(node_infos) = self.store.node_infos_by_sync_topics(&[topic]).await else {
            return;
        };

        // Remove ourselves.
        let node_infos = node_infos
            .into_iter()
            .filter(|info| info.id() != self.args.public_key)
            .collect();

        if tx.send(TopicEvent { topic, node_infos }).is_err() {
            // On an error we know that all receivers have been dropped, so we can remove this
            // subscription as well and clean up after ourselves.
            self.topic_subscribers.remove(&topic);
        }
    }

    async fn call_ephemeral_topic_subscribers(&mut self, topic: TopicId) {
        let Some(tx) = self.topic_subscribers.get(&topic) else {
            return;
        };

        let Ok(node_infos) = self
            .store
            .node_infos_by_ephemeral_messaging_topics(&[topic])
            .await
        else {
            return;
        };

        // Remove ourselves.
        let node_infos = node_infos
            .into_iter()
            .filter(|info| info.id() != self.args.public_key)
            .collect();

        if tx.send(TopicEvent { topic, node_infos }).is_err() {
            // On an error we know that all receivers have been dropped, so we can remove this
            // subscription as well and clean up after ourselves.
            self.topic_subscribers.remove(&topic);
        }
    }
}

pub struct AddressBook<S> {
    _marker: PhantomData<S>,
}

impl<S> Default for AddressBook<S> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<S> ThreadLocalActor for AddressBook<S>
where
    S: AddressBookStore<NodeId, NodeInfo> + Send + 'static,
    S::Error: StdError + Send + Sync + 'static,
{
    type State = AddressBookState<S>;

    type Msg = ToAddressBook;

    // @TODO: For now we leave out the concept of a `NetworkId` but we may want some way to slice
    // address subsets in the future.
    type Arguments = (ApplicationArguments, S);

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let (args, store) = args;
        Ok(AddressBookState {
            args,
            store,
            node_subscribers: HashMap::new(),
            topic_subscribers: HashMap::new(),
        })
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Note that critical storage failures will return an `ActorProcessingErr` and cause this
        // actor to restart when supervised.
        match message {
            ToAddressBook::InsertNodeInfo(node_info, reply) => {
                // Check signature of information. Is it authentic?
                if let Err(err) = node_info.verify().map_err(AddressBookError::NodeInfo) {
                    let _ = reply.send(Err(err));
                    return Ok(());
                }

                state.call_node_subscribers(node_info.id(), &node_info);

                // Overwrite any previously given information if it existed.
                let result = state.store.insert_node_info(node_info).await?;

                let _ = reply.send(Ok(result));
            }
            ToAddressBook::InsertTransportInfo(node_id, transport_info, reply) => {
                // Check signature of information. Is it authentic?
                if let Err(err) = transport_info
                    .verify(&node_id)
                    .map_err(AddressBookError::NodeInfo)
                {
                    let _ = reply.send(Err(err));
                    return Ok(());
                }

                // Is there already an existing entry? Only replace it when information is newer
                // (it's a simple "last write wins" principle based on a UNIX timestamp) handled
                // inside of `update_transports`.
                //
                // If a node info already exists, only update the "transports" aspect of it and
                // keep any other "local" configuration, otherwise create a new "default" node info.
                let mut node_info = match state.store.node_info(&node_id).await? {
                    Some(current) => current,
                    None => NodeInfo::new(node_id),
                };

                match node_info.update_transports(transport_info) {
                    Ok(is_newer) => {
                        if is_newer {
                            state.call_node_subscribers(node_info.id(), &node_info);
                        }

                        state.store.insert_node_info(node_info).await?;

                        let _ = reply.send(Ok(is_newer));
                    }
                    Err(err) => {
                        let _ = reply.send(Err(AddressBookError::NodeInfo(err)));
                    }
                }
            }
            ToAddressBook::SubscribeNodeChanges(node_id, reply) => {
                let rx = match state.node_subscribers.get_mut(&node_id) {
                    Some(tx) => tx.subscribe(),
                    None => {
                        let (tx, rx) = broadcast::channel(32);
                        state.node_subscribers.insert(node_id, tx);
                        rx
                    }
                };
                let _ = reply.send(rx);
            }
            ToAddressBook::SubscribeTopicChanges(topic, reply) => {
                let rx = match state.topic_subscribers.get_mut(&topic) {
                    Some(tx) => tx.subscribe(),
                    None => {
                        let (tx, rx) = broadcast::channel(32);
                        state.topic_subscribers.insert(topic, tx);
                        rx
                    }
                };
                let _ = reply.send(rx);
            }

            // Mostly a wrapper around the store ..
            ToAddressBook::NodeInfo(node_id, reply) => {
                let result = state.store.node_info(&node_id).await?;
                let _ = reply.send(result);
            }
            ToAddressBook::AllNodeInfos(reply) => {
                let result = state.store.all_node_infos().await?;
                let _ = reply.send(result);
            }
            ToAddressBook::SelectedNodeInfos(node_ids, reply) => {
                let result = state.store.selected_node_infos(&node_ids).await?;
                let _ = reply.send(result);
            }
            ToAddressBook::NodeInfosBySyncTopics(topics, reply) => {
                let result = state.store.node_infos_by_sync_topics(&topics).await?;
                // Remove ourselves.
                let result = result
                    .into_iter()
                    .filter(|info| info.id() != state.args.public_key)
                    .collect();
                let _ = reply.send(result);
            }
            ToAddressBook::NodeInfosByEphemeralMessagingTopics(topics, reply) => {
                let result = state
                    .store
                    .node_infos_by_ephemeral_messaging_topics(&topics)
                    .await?;
                // Remove ourselves.
                let result = result
                    .into_iter()
                    .filter(|info| info.id() != state.args.public_key)
                    .collect();
                let _ = reply.send(result);
            }
            ToAddressBook::RandomNodeInfo(reply) => {
                let result = state.store.random_node().await?;
                let _ = reply.send(result);
            }
            ToAddressBook::SetSyncTopics(node_id, topics) => {
                info!("set {} sync topic(s) for node {}", topics.len(), node_id.to_hex());
                for topic in &topics {
                    state.call_sync_topic_subscribers(*topic).await;
                }

                state.store.set_sync_topics(node_id, topics).await?;
            }
            ToAddressBook::SetEphemeralMessagingTopics(node_id, topics) => {
                info!("set {} ephemeral topic(s) for node {}", topics.len(), node_id.to_hex());
                for topic in &topics {
                    state.call_ephemeral_topic_subscribers(*topic).await;
                }

                state
                    .store
                    .set_ephemeral_messaging_topics(node_id, topics)
                    .await?;
            }
            ToAddressBook::RemoveNodeInfo(node_id, reply) => {
                let result = state.store.remove_node_info(&node_id).await?;
                let _ = reply.send(result);
            }
            ToAddressBook::RemoveOlderThan(duration, reply) => {
                let result = state.store.remove_older_than(duration).await?;
                let _ = reply.send(result);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct TopicEvent {
    #[allow(unused)]
    pub topic: TopicId,
    pub node_infos: Vec<NodeInfo>,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct NodeEvent {
    pub node_id: NodeId,
    pub node_info: NodeInfo,
}

#[derive(Debug, Error)]
pub enum AddressBookError {
    #[error(transparent)]
    NodeInfo(crate::addrs::NodeInfoError),
}

#[cfg(test)]
mod tests {
    use p2panda_core::PrivateKey;
    use p2panda_discovery::address_book::NodeInfo as _;
    use ractor::call;
    use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

    use crate::actors::{generate_actor_namespace, with_namespace};
    use crate::addrs::{NodeInfo, TransportAddress, UnsignedTransportInfo};
    use crate::test_utils::test_args;

    use super::{ADDRESS_BOOK, AddressBook, ToAddressBook};

    #[tokio::test]
    async fn insert_node_and_transport_info() {
        let (args, store, _) = test_args();

        let actor_namespace = generate_actor_namespace(&args.public_key);
        let spawner = ThreadLocalActorSpawner::new();

        let (actor, _handle) = AddressBook::spawn(
            Some(with_namespace(ADDRESS_BOOK, &actor_namespace)),
            (args.clone(), store),
            spawner,
        )
        .await
        .unwrap();

        // Insert new node info.
        let node_info = NodeInfo::new(args.public_key.clone());
        let result = call!(actor, ToAddressBook::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Overwriting node info should return "false".
        let mut node_info = NodeInfo::new(args.public_key.clone());
        node_info.bootstrap = true;
        let result = call!(actor, ToAddressBook::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Bootstrap should be set to "true", as node info was still overwritten.
        let result = call!(actor, ToAddressBook::NodeInfo, args.public_key.clone())
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
                    transport_info.timestamp = 1234; // Manipulate timestamp to make signature invalid
                    transport_info
                }),
            }
        };
        assert!(node_info.verify().is_err());
        let result = call!(actor, ToAddressBook::InsertNodeInfo, node_info).unwrap();
        assert!(result.is_err());

        // Inserting transport info should not overwrite "local" data.
        let mut node_info = NodeInfo::new(args.public_key.clone());
        node_info.bootstrap = true;
        let result = call!(actor, ToAddressBook::InsertNodeInfo, node_info).unwrap();
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
            ToAddressBook::InsertTransportInfo,
            args.public_key.clone(),
            transport_info
        )
        .unwrap();
        assert!(result.is_ok());

        // Even after insertion of new transport info, the "local" bootstrap config is still true.
        let result = call!(actor, ToAddressBook::NodeInfo, args.public_key.clone())
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
            transport_info.timestamp = 1234; // Manipulate timestamp to make signature invalid
            transport_info
        };
        assert!(transport_info.verify(&args.public_key).is_err());
        let result = call!(
            actor,
            ToAddressBook::InsertTransportInfo,
            args.public_key.clone(),
            transport_info
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
            ToAddressBook::InsertTransportInfo,
            public_key,
            transport_info
        )
        .unwrap();
        assert!(result.is_ok());

        let result = call!(actor, ToAddressBook::NodeInfo, public_key)
            .unwrap()
            .expect("node info exists in store");
        assert!(!result.bootstrap);
        assert!(result.transports().is_some());
    }
}
