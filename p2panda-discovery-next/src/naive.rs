// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::address_book::{AddressBookStore, NodeInfo};
use crate::traits::{DiscoveryProtocol, DiscoveryResult, SubscriptionInfo};

#[derive(Serialize, Deserialize)]
pub enum NaiveDiscoveryMessage<T, ID, N>
where
    N: NodeInfo<ID>,
    for<'a> N::Transports: Serialize + Deserialize<'a>,
    T: Eq + StdHash,
    ID: Ord,
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
    for<'a> N::Transports: Serialize + Deserialize<'a>,
{
    type Error = NaiveDiscoveryError<S, P, T, ID, N>;

    type Message = NaiveDiscoveryMessage<T, ID, N>;

    async fn alice(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
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
        .await
        .map_err(|_| NaiveDiscoveryError::Sink)?;

        // 2. Alice receives Bob's topics and topic ids.
        let Some(Ok(message)) = rx.next().await else {
            return Err(NaiveDiscoveryError::Stream);
        };
        let NaiveDiscoveryMessage::Topics {
            topics: remote_topics,
            topic_ids: remote_topic_ids,
        } = message
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
        .await
        .map_err(|_| NaiveDiscoveryError::Sink)?;

        // 4. Alice receives Bob's node infos.
        let Some(Ok(message)) = rx.next().await else {
            return Err(NaiveDiscoveryError::Stream);
        };
        let NaiveDiscoveryMessage::Nodes {
            transport_infos: remote_transport_infos,
        } = message
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
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<DiscoveryResult<T, ID, N>, Self::Error> {
        // 1. Bob receives Alice's topics and topic ids.
        let Some(Ok(message)) = rx.next().await else {
            return Err(NaiveDiscoveryError::Stream);
        };
        let NaiveDiscoveryMessage::Topics {
            topics: remote_topics,
            topic_ids: remote_topic_ids,
        } = message
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
        .await
        .map_err(|_| NaiveDiscoveryError::Sink)?;

        // 3. Bob receives Alice's node infos.
        let Some(Ok(message)) = rx.next().await else {
            return Err(NaiveDiscoveryError::Stream);
        };
        let NaiveDiscoveryMessage::Nodes {
            transport_infos: remote_transport_infos,
        } = message
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
        .await
        .map_err(|_| NaiveDiscoveryError::Sink)?;

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
    T: Eq + StdHash,
    ID: Clone + Ord,
    N: NodeInfo<ID>,
    for<'a> N::Transports: Serialize + Deserialize<'a>,
{
    #[error("{0}")]
    Store(S::Error),

    #[error("{0}")]
    Subscription(P::Error),

    #[error("received unexpected message")]
    UnexpectedMessage,

    #[error("stream closed unexpectedly")]
    Stream,

    #[error("sink closed unexpectedly")]
    Sink,
}
