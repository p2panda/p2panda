// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::HashSet;
use std::error::Error as StdError;
use std::marker::PhantomData;
use std::time::Duration;

// @TODO: This will come from `p2panda-store` eventually.
use p2panda_discovery::address_book::{AddressBookStore, NodeInfo as _};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef, RpcReplyPort};
use thiserror::Error;
use tracing::{debug, warn};

use crate::actors::address_book::watchers::{
    UpdateResult, UpdatesOnly, Watched, WatchedValue, WatcherReceiver, WatcherSet,
};
use crate::args::ApplicationArguments;
use crate::utils::ShortFormat;
use crate::{NodeId, NodeInfo, TopicId, TransportInfo};

/// Address book actor name.
pub const ADDRESS_BOOK: &str = "net.address_book";

pub enum ToAddressBook {
    /// Returns information about a node.
    ///
    /// Returns `None` if no information was found for this node.
    NodeInfo(NodeId, RpcReplyPort<Option<NodeInfo>>),

    /// Returns a list of informations about nodes which are all interested in at least one of the
    /// given topics in this set.
    #[allow(unused)]
    NodeInfosBySyncTopics(Vec<TopicId>, RpcReplyPort<Vec<NodeInfo>>),

    /// Returns a list of informations about nodes which are all interested in at least one of the
    /// given topics in this set.
    NodeInfosByEphemeralMessagingTopics(Vec<TopicId>, RpcReplyPort<Vec<NodeInfo>>),

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

    /// Subscribes to channel informing us about node info changes for a specific node.
    WatchNodeInfo(
        NodeId,
        UpdatesOnly,
        RpcReplyPort<WatcherReceiver<Option<NodeInfo>>>,
    ),

    /// Subscribes to channel informing us about changes of the set of nodes interested in a topic
    /// for eventually consistent and ephemeral streams.
    WatchTopic(
        TopicId,
        UpdatesOnly,
        RpcReplyPort<WatcherReceiver<HashSet<NodeId>>>,
    ),
}

pub struct AddressBookState<S> {
    args: ApplicationArguments,
    store: S,
    node_watchers: WatcherSet<NodeId, WatchedNodeInfo>,
    topic_watchers: WatcherSet<TopicId, WatchedTopic>,
}

impl<S> AddressBookState<S>
where
    S: AddressBookStore<NodeId, NodeInfo>,
{
    async fn node_infos_by_sync_topics(
        &self,
        topics: Vec<TopicId>,
    ) -> Result<Vec<NodeInfo>, S::Error> {
        let result = self.store.node_infos_by_sync_topics(&topics).await?;

        // Remove ourselves.
        let result = result
            .into_iter()
            .filter(|info| info.id() != self.args.public_key)
            .collect();

        Ok(result)
    }

    async fn node_infos_by_ephemeral_messaging_topics(
        &self,
        topics: Vec<TopicId>,
    ) -> Result<Vec<NodeInfo>, S::Error> {
        let result = self
            .store
            .node_infos_by_ephemeral_messaging_topics(&topics)
            .await?;

        // Remove ourselves.
        let result = result
            .into_iter()
            .filter(|info| info.id() != self.args.public_key)
            .collect();

        Ok(result)
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
            node_watchers: WatcherSet::new(),
            topic_watchers: WatcherSet::new(),
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

                // Overwrite any previously given information if it existed.
                let result = state.store.insert_node_info(node_info.clone()).await?;

                // Inform subscribers about this update. This will only get notified if it really
                // changed.
                state
                    .node_watchers
                    .update(&node_info.node_id, Some(node_info.clone()));

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
                        state.store.insert_node_info(node_info.clone()).await?;
                        let _ = reply.send(Ok(is_newer));
                    }
                    Err(err) => {
                        let _ = reply.send(Err(AddressBookError::NodeInfo(err)));
                    }
                }

                // Inform subscribers about this update. This will only get notified if it really
                // changed.
                state
                    .node_watchers
                    .update(&node_info.node_id, Some(node_info.clone()));
            }
            ToAddressBook::WatchNodeInfo(node_id, updates_only, reply) => {
                let node_info = state.store.node_info(&node_id).await?;
                let rx = state.node_watchers.subscribe(
                    node_id,
                    updates_only,
                    WatchedNodeInfo::from_node_info(node_info),
                );
                let _ = reply.send(rx);
            }
            ToAddressBook::WatchTopic(topic, updates_only, reply) => {
                // Since we don't know where this topic belongs to we need to check both stream
                // types.
                let sync_node_ids: HashSet<NodeId> = state
                    .node_infos_by_sync_topics(vec![topic])
                    .await?
                    .iter()
                    .map(|info| info.id())
                    .collect();

                let ephemeral_node_ids: HashSet<NodeId> = state
                    .node_infos_by_ephemeral_messaging_topics(vec![topic])
                    .await?
                    .iter()
                    .map(|info| info.id())
                    .collect();

                // @TODO: Topic re-use across stream types should be prohibited on our high-level
                // API and discovery protocol.
                if !sync_node_ids.is_empty() && !ephemeral_node_ids.is_empty() {
                    warn!(
                        topic = %topic.fmt_short(),
                        "detected re-use of the same topic for both ephemeral messaging and sync"
                    );
                    return Ok(());
                }

                // Since we've checked that one of the sets _needs_ to be empty, we can simply
                // merge them.
                let mut node_ids: HashSet<NodeId> = sync_node_ids;
                node_ids.extend(ephemeral_node_ids);

                let rx = state.topic_watchers.subscribe(
                    topic,
                    updates_only,
                    WatchedTopic::from_node_ids(topic, node_ids),
                );
                let _ = reply.send(rx);
            }

            // Mostly a wrapper around the store ..
            ToAddressBook::NodeInfo(node_id, reply) => {
                let result = state.store.node_info(&node_id).await?;
                let _ = reply.send(result);
            }
            ToAddressBook::NodeInfosBySyncTopics(topics, reply) => {
                let result = state.node_infos_by_sync_topics(topics).await?;
                let _ = reply.send(result);
            }
            ToAddressBook::NodeInfosByEphemeralMessagingTopics(topics, reply) => {
                let result = state
                    .node_infos_by_ephemeral_messaging_topics(topics)
                    .await?;
                let _ = reply.send(result);
            }
            ToAddressBook::SetSyncTopics(node_id, topics) => {
                state.store.set_sync_topics(node_id, topics.clone()).await?;

                // Inform subscribers about potential change in set of interested nodes.
                for topic in &topics {
                    let node_ids = state
                        .node_infos_by_sync_topics(vec![*topic])
                        .await?
                        .into_iter()
                        .map(|info| info.id());
                    state
                        .topic_watchers
                        .update(topic, HashSet::from_iter(node_ids));
                }
            }
            ToAddressBook::SetEphemeralMessagingTopics(node_id, topics) => {
                state
                    .store
                    .set_ephemeral_messaging_topics(node_id, topics.clone())
                    .await?;

                // Inform subscribers about potential change in set of interested nodes.
                for topic in &topics {
                    let node_ids = state
                        .node_infos_by_ephemeral_messaging_topics(vec![*topic])
                        .await?
                        .into_iter()
                        .map(|info| info.id());
                    state
                        .topic_watchers
                        .update(topic, HashSet::from_iter(node_ids));
                }
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

#[derive(Debug, Error)]
pub enum AddressBookError {
    #[error(transparent)]
    NodeInfo(crate::addrs::NodeInfoError),
}

#[derive(Default)]
pub struct WatchedNodeInfo(RefCell<Option<NodeInfo>>);

impl WatchedNodeInfo {
    pub fn from_node_info(node_info: Option<NodeInfo>) -> Self {
        Self(RefCell::new(node_info))
    }
}

impl Watched for WatchedNodeInfo {
    type Value = Option<NodeInfo>;

    fn current(&self) -> Self::Value {
        self.0.borrow().clone()
    }

    fn update_if_changed(&self, cmp: &Self::Value) -> UpdateResult<Self::Value> {
        if !self.0.borrow().eq(cmp) {
            if let Some(info) = cmp {
                let transports = info
                    .transports
                    .as_ref()
                    .map(|info| info.to_string())
                    .unwrap_or("none".to_string());
                debug!(
                    node_id = info.node_id.fmt_short(),
                    %transports,
                    "node info changed"
                );
            }

            self.0.replace(cmp.to_owned());

            UpdateResult::Changed(WatchedValue {
                difference: None,
                value: cmp.to_owned(),
            })
        } else {
            UpdateResult::Unchanged
        }
    }
}

pub struct WatchedTopic {
    topic: TopicId,
    node_ids: RefCell<HashSet<NodeId>>,
}

impl WatchedTopic {
    pub fn from_node_ids(topic: TopicId, node_ids: HashSet<NodeId>) -> Self {
        Self {
            topic,
            node_ids: RefCell::new(node_ids),
        }
    }
}

impl Watched for WatchedTopic {
    type Value = HashSet<NodeId>;

    fn current(&self) -> Self::Value {
        self.node_ids.borrow().clone()
    }

    fn update_if_changed(&self, cmp: &Self::Value) -> UpdateResult<Self::Value> {
        let difference: HashSet<NodeId> = self
            .node_ids
            .borrow()
            .symmetric_difference(cmp)
            .cloned()
            .collect();

        if difference.is_empty() {
            UpdateResult::Unchanged
        } else {
            {
                let node_ids: Vec<String> = self
                    .node_ids
                    .borrow()
                    .iter()
                    .map(|id| id.fmt_short())
                    .collect();
                debug!(
                    topic = self.topic.fmt_short(),
                    node_ids = ?node_ids,
                    "interested nodes for topic changed"
                );
            }

            self.node_ids.replace(cmp.to_owned());

            UpdateResult::Changed(WatchedValue {
                difference: Some(difference),
                value: cmp.to_owned(),
            })
        }
    }
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
