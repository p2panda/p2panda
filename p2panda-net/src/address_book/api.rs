// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

use p2panda_discovery::address_book::{BoxedAddressBookStore, BoxedError};
use ractor::{ActorRef, call, cast};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::address_book::Builder;
use crate::address_book::actor::ToAddressBookActor;
use crate::address_book::report::ConnectionOutcome;
use crate::addrs::{NodeInfo, NodeInfoError, TransportInfo};
use crate::watchers::{UpdatesOnly, WatcherReceiver};
use crate::{NodeId, TopicId};

#[derive(Clone)]
pub struct AddressBook {
    pub(super) inner: Arc<RwLock<Inner>>,
}

pub(super) struct Inner {
    pub(super) actor_ref: Option<ActorRef<ToAddressBookActor>>,
}

impl AddressBook {
    pub(crate) fn new(actor_ref: Option<ActorRef<ToAddressBookActor>>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
        }
    }

    pub fn builder() -> Builder {
        Builder::new()
    }

    /// Returns information about a node.
    ///
    /// Returns `None` if no information was found for this node.
    pub async fn node_info(&self, node_id: NodeId) -> Result<Option<NodeInfo>, AddressBookError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::NodeInfo,
            node_id
        )
        .map_err(Box::new)?;
        Ok(result)
    }

    /// Inserts or updates node information into address book.
    ///
    /// Use this method if adding node information from a local configuration or trusted, external
    /// source, etc.
    ///
    /// Returns `true` if entry got newly inserted or `false` if existing entry was updated.
    /// Previous entries are simply overwritten. Entries with attached transport information get
    /// checked against authenticity and throw an error otherwise.
    pub async fn insert_node_info(&self, node_info: NodeInfo) -> Result<bool, AddressBookError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::InsertNodeInfo,
            node_info
        )
        .map_err(Box::new)??;
        Ok(result)
    }

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
    pub async fn insert_transport_info(
        &self,
        node_id: NodeId,
        transport_info: TransportInfo,
    ) -> Result<bool, AddressBookError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::InsertTransportInfo,
            node_id,
            transport_info
        )
        .map_err(Box::new)??;
        Ok(result)
    }

    pub async fn node_infos_by_topics(
        &self,
        topics: impl IntoIterator<Item = TopicId>,
    ) -> Result<Vec<NodeInfo>, AddressBookError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::NodeInfosByTopics,
            topics.into_iter().collect()
        )
        .map_err(Box::new)?;
        Ok(result)
    }

    pub async fn set_topics(
        &self,
        node_id: NodeId,
        topics: impl IntoIterator<Item = TopicId>,
    ) -> Result<(), AddressBookError> {
        let inner = self.inner.read().await;
        cast!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::SetTopics(node_id, topics.into_iter().collect())
        )
        .map_err(Box::new)?;
        Ok(())
    }

    pub async fn add_topic(&self, node_id: NodeId, topic: TopicId) -> Result<(), AddressBookError> {
        let inner = self.inner.read().await;
        cast!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::AddTopic(node_id, topic)
        )
        .map_err(Box::new)?;
        Ok(())
    }

    pub async fn remove_topic(
        &self,
        node_id: NodeId,
        topic: TopicId,
    ) -> Result<(), AddressBookError> {
        let inner = self.inner.read().await;
        cast!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::RemoveTopic(node_id, topic)
        )
        .map_err(Box::new)?;
        Ok(())
    }

    /// Subscribes to channel informing us about node info changes for a specific node.
    pub async fn watch_node_info(
        &self,
        node_id: NodeId,
        updates_only: UpdatesOnly,
    ) -> Result<WatcherReceiver<Option<NodeInfo>>, AddressBookError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::WatchNodeInfo,
            node_id,
            updates_only
        )
        .map_err(Box::new)?;
        Ok(result)
    }

    /// Subscribes to channel informing us about changes of the set of nodes interested in a topic.
    pub async fn watch_topic(
        &self,
        topic_id: TopicId,
        updates_only: UpdatesOnly,
    ) -> Result<WatcherReceiver<HashSet<NodeId>>, AddressBookError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::WatchTopic,
            topic_id,
            updates_only
        )
        .map_err(Box::new)?;
        Ok(result)
    }

    /// Subscribes to channel informing us about topic changes for a particular node.
    pub async fn watch_node_topics(
        &self,
        node_id: NodeId,
        updates_only: UpdatesOnly,
    ) -> Result<WatcherReceiver<HashSet<TopicId>>, AddressBookError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::WatchNodeTopics,
            node_id,
            updates_only
        )
        .map_err(Box::new)?;
        Ok(result)
    }

    /// Report outcomes of incoming or outgoing connections.
    ///
    /// This helps measuring the "quality" of nodes which will be recorded in the address book.
    pub async fn report(
        &self,
        node_id: NodeId,
        connection_outcome: ConnectionOutcome,
    ) -> Result<(), AddressBookError> {
        let inner = self.inner.read().await;
        cast!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::Report(node_id, connection_outcome)
        )
        .map_err(Box::new)?;
        Ok(())
    }

    pub(crate) async fn store(
        &self,
    ) -> Result<BoxedAddressBookStore<NodeId, NodeInfo>, AddressBookError> {
        let inner = self.inner.read().await;
        let result = call!(
            inner.actor_ref.as_ref().expect("actor spawned in builder"),
            ToAddressBookActor::Store
        )
        .map_err(Box::new)?;
        Ok(result)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Some(actor_ref) = self.actor_ref.take() {
            actor_ref.stop(None);
        }
    }
}

impl std::fmt::Debug for AddressBook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AddressBook").finish()
    }
}

#[derive(Debug, Error)]
pub enum AddressBookError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToAddressBookActor>>),

    /// Spawning the internal actor as a child actor of a supervisor failed.
    #[cfg(feature = "supervisor")]
    #[error(transparent)]
    SupervisorSpawn(#[from] crate::supervisor::SupervisorError),

    /// Address book store failed.
    #[error(transparent)]
    Store(#[from] BoxedError),

    /// Invalid node info provided.
    #[error(transparent)]
    NodeInfo(#[from] NodeInfoError),
}
