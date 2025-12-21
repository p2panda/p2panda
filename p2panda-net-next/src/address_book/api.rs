// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_discovery::address_book::AddressBookStore;
use ractor::{ActorRef, call, cast};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::address_book::Builder;
use crate::address_book::actor::ToAddressBookActor;
use crate::address_book::report::ConnectionOutcome;
use crate::addrs::{NodeId, NodeInfo};
use crate::watchers::{UpdatesOnly, WatcherReceiver};
use crate::{NodeInfoError, TopicId};

#[derive(Clone)]
pub struct AddressBook<S> {
    pub(crate) actor_ref: Arc<RwLock<ActorRef<ToAddressBookActor>>>,
    pub(crate) _marker: PhantomData<S>,
}

impl<S> AddressBook<S>
where
    S: AddressBookStore<NodeId, NodeInfo>,
{
    // TODO(adz): Can we remove the node id argument here? We need it in the address book only to
    // remove ourselves from some results, but maybe that can be handled somewhere else?
    pub fn builder(my_id: NodeId, store: S) -> Builder<S> {
        Builder::<S> { my_id, store }
    }

    /// Returns information about a node.
    ///
    /// Returns `None` if no information was found for this node.
    pub async fn node_info(
        &self,
        node_id: NodeId,
    ) -> Result<Option<NodeInfo>, AddressBookError<S>> {
        let actor_ref = self.actor_ref.read().await;
        let result = call!(actor_ref, ToAddressBookActor::NodeInfo, node_id)?;
        Ok(result)
    }

    /// Inserts or updates node information into address book.
    ///
    /// Use this method if adding node information from a local configuration, trusted, external
    /// source, etc.
    ///
    /// Returns `true` if entry got newly inserted or `false` if existing entry was updated.
    /// Previous entries are simply overwritten. Entries with attached transport information get
    /// checked against authenticity and throw an error otherwise.
    pub async fn insert_node_info(&self, node_info: NodeInfo) -> Result<bool, AddressBookError<S>> {
        let actor_ref = self.actor_ref.read().await;
        let result = call!(actor_ref, ToAddressBookActor::InsertNodeInfo, node_info)??;
        Ok(result)
    }

    /// Subscribes to channel informing us about node info changes for a specific node.
    pub async fn watch_node_info(
        &self,
        node_id: NodeId,
        updates_only: UpdatesOnly,
    ) -> Result<WatcherReceiver<Option<NodeInfo>>, AddressBookError<S>> {
        let actor_ref = self.actor_ref.read().await;
        let result = call!(
            actor_ref,
            ToAddressBookActor::WatchNodeInfo,
            node_id,
            updates_only
        )?;
        Ok(result)
    }

    /// Subscribes to channel informing us about changes of the set of nodes interested in a topic
    /// for eventually consistent and ephemeral streams.
    pub async fn watch_topic(
        &self,
        topic_id: TopicId,
        updates_only: UpdatesOnly,
    ) -> Result<WatcherReceiver<HashSet<NodeId>>, AddressBookError<S>> {
        let actor_ref = self.actor_ref.read().await;
        let result = call!(
            actor_ref,
            ToAddressBookActor::WatchTopic,
            topic_id,
            updates_only
        )?;
        Ok(result)
    }

    /// Subscribes to channel informing us about topic changes for a particular node.
    pub async fn watch_node_topics(
        &self,
        node_id: NodeId,
        updates_only: UpdatesOnly,
    ) -> Result<WatcherReceiver<HashSet<TopicId>>, AddressBookError<S>> {
        let actor_ref = self.actor_ref.read().await;
        let result = call!(
            actor_ref,
            ToAddressBookActor::WatchNodeTopics,
            node_id,
            updates_only
        )?;
        Ok(result)
    }

    /// Report outcomes of incoming or outgoing connections.
    ///
    /// This helps measuring the "quality" of nodes which will be recorded in the address book.
    pub async fn report(
        &self,
        node_id: NodeId,
        connection_outcome: ConnectionOutcome,
    ) -> Result<(), AddressBookError<S>> {
        let actor_ref = self.actor_ref.read().await;
        cast!(
            actor_ref,
            ToAddressBookActor::Report(node_id, connection_outcome)
        )?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AddressBookError<S>
where
    S: AddressBookStore<NodeId, NodeInfo>,
{
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] ractor::RactorErr<ToAddressBookActor>),

    /// Address book store failed.
    #[error("{0}")]
    Store(S::Error),

    /// Invalid node info provided.
    #[error(transparent)]
    NodeInfo(#[from] NodeInfoError),
}
