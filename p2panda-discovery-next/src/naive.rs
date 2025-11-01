// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use thiserror::Error;
use tokio::sync::mpsc;

use crate::address_book::{AddressBookStore, NodeInfo};
use crate::traits::{DiscoveryProtocol, DiscoveryResult, Receiver, Sender, SubscriptionInfo};

pub enum NaiveDiscoveryMessage<T, ID, N>
where
    N: NodeInfo<ID>,
{
    Topics {
        topics: HashSet<T>,
        topic_ids: HashSet<[u8; 32]>,
    },
    Nodes {
        transport_infos: BTreeMap<ID, N::Transports>,
    },
}

pub struct NaiveDiscoveryProtocol<S, P, T, ID, N> {
    store: S,
    subscription: P,
    remote_node_id: ID,
    _marker: PhantomData<(T, N)>,
}

impl<S, P, T, ID, N> NaiveDiscoveryProtocol<S, P, T, ID, N> {
    pub fn new(store: S, subscription: P, remote_node_id: ID) -> Self {
        Self {
            store,
            subscription,
            remote_node_id,
            _marker: PhantomData,
        }
    }
}

impl<S, P, T, ID, N> DiscoveryProtocol<T, ID, N> for NaiveDiscoveryProtocol<S, P, T, ID, N>
where
    S: AddressBookStore<T, ID, N>,
    P: SubscriptionInfo<T>,
    T: Eq + StdHash,
    ID: Clone + Ord,
    N: NodeInfo<ID>,
{
    type Error = NaiveDiscoveryError<S, P, T, ID, N>;

    type Message = NaiveDiscoveryMessage<T, ID, N>;

    async fn alice(
        &self,
        tx: Sender<Self::Message>,
        mut rx: Receiver<Self::Message>,
    ) -> Result<DiscoveryResult<T, ID, N>, Self::Error> {
        // 1. Alice sends Bob all their topics and topic ids.
        let my_topics = self
            .subscription
            .subscribed_topics()
            .await
            .map_err(NaiveDiscoveryError::Subscription)?;

        let my_topic_ids = self
            .subscription
            .subscribed_topic_ids()
            .await
            .map_err(NaiveDiscoveryError::Subscription)?;

        tx.send(NaiveDiscoveryMessage::Topics {
            topics: HashSet::from_iter(my_topics.into_iter()),
            topic_ids: HashSet::from_iter(my_topic_ids.into_iter()),
        })
        .await?;

        // 2. Alice receives Bob's topics and topic ids.
        let Some(NaiveDiscoveryMessage::Topics {
            topics: remote_topics,
            topic_ids: remote_topic_ids,
        }) = rx.recv().await
        else {
            return Err(NaiveDiscoveryError::UnexpectedMessage);
        };

        // 3. Alice sends Bob all node infos they know about.
        let node_infos = self
            .store
            .all_node_infos()
            .await
            .map_err(NaiveDiscoveryError::Store)?;

        tx.send(NaiveDiscoveryMessage::Nodes {
            transport_infos: {
                let mut map = BTreeMap::new();
                for node_info in node_infos {
                    if let Some(transport_info) = node_info.transports() {
                        map.insert(node_info.id(), transport_info);
                    }
                }
                map
            },
        })
        .await?;

        // 4. Alice receives Bob's node infos.
        let Some(NaiveDiscoveryMessage::Nodes {
            transport_infos: remote_transport_infos,
        }) = rx.recv().await
        else {
            return Err(NaiveDiscoveryError::UnexpectedMessage);
        };

        Ok(DiscoveryResult {
            remote_node_id: self.remote_node_id.clone(),
            node_transport_infos: remote_transport_infos,
            node_topics: remote_topics,
            node_topic_ids: remote_topic_ids,
        })
    }

    async fn bob(
        &self,
        tx: Sender<Self::Message>,
        mut rx: Receiver<Self::Message>,
    ) -> Result<DiscoveryResult<T, ID, N>, Self::Error> {
        // 1. Bob receives Alice's topics and topic ids.
        let Some(NaiveDiscoveryMessage::Topics {
            topics: remote_topics,
            topic_ids: remote_topic_ids,
        }) = rx.recv().await
        else {
            return Err(NaiveDiscoveryError::UnexpectedMessage);
        };

        // 2. Bob sends Alice all their topics and topic ids.
        let my_topics = self
            .subscription
            .subscribed_topics()
            .await
            .map_err(NaiveDiscoveryError::Subscription)?;

        let my_topic_ids = self
            .subscription
            .subscribed_topic_ids()
            .await
            .map_err(NaiveDiscoveryError::Subscription)?;

        tx.send(NaiveDiscoveryMessage::Topics {
            topics: HashSet::from_iter(my_topics.into_iter()),
            topic_ids: HashSet::from_iter(my_topic_ids.into_iter()),
        })
        .await?;

        // 3. Bob receives Alice's node infos.
        let Some(NaiveDiscoveryMessage::Nodes {
            transport_infos: remote_transport_infos,
        }) = rx.recv().await
        else {
            return Err(NaiveDiscoveryError::UnexpectedMessage);
        };

        // 4. Bob sends Alice all node infos they know about.
        let node_infos = self
            .store
            .all_node_infos()
            .await
            .map_err(NaiveDiscoveryError::Store)?;

        tx.send(NaiveDiscoveryMessage::Nodes {
            transport_infos: {
                let mut map = BTreeMap::new();
                for node_info in node_infos {
                    if let Some(transport_info) = node_info.transports() {
                        map.insert(node_info.id(), transport_info);
                    }
                }
                map
            },
        })
        .await?;

        Ok(DiscoveryResult {
            remote_node_id: self.remote_node_id.clone(),
            node_transport_infos: remote_transport_infos,
            node_topics: remote_topics,
            node_topic_ids: remote_topic_ids,
        })
    }
}

#[derive(Debug, Error)]
pub enum NaiveDiscoveryError<S, P, T, ID, N>
where
    S: AddressBookStore<T, ID, N>,
    P: SubscriptionInfo<T>,
    N: NodeInfo<ID>,
{
    #[error("{0}")]
    Store(S::Error),

    #[error("{0}")]
    Subscription(P::Error),

    #[error(transparent)]
    Sender(#[from] mpsc::error::SendError<NaiveDiscoveryMessage<T, ID, N>>),

    #[error("received unexpected message")]
    UnexpectedMessage,
}
