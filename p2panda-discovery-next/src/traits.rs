// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};

use tokio::sync::mpsc;

use crate::address_book::NodeInfo;

pub trait DiscoveryStrategy<N> {
    type Error;

    fn next_node(&self) -> impl Future<Output = Result<Option<N>, Self::Error>>;
}

pub trait DiscoveryProtocol<T, ID, N>
where
    N: NodeInfo<ID>,
{
    type Error;

    type Message;

    fn alice(
        &self,
        tx: Sender<Self::Message>,
        rx: Receiver<Self::Message>,
    ) -> impl Future<Output = Result<DiscoveryResult<T, ID, N>, Self::Error>>;

    fn bob(
        &self,
        tx: Sender<Self::Message>,
        rx: Receiver<Self::Message>,
    ) -> impl Future<Output = Result<DiscoveryResult<T, ID, N>, Self::Error>>;
}

#[derive(Clone, Debug)]
pub struct DiscoveryResult<T, ID, N>
where
    N: NodeInfo<ID>,
{
    pub remote_node_id: ID,
    pub node_transport_infos: BTreeMap<ID, N::Transports>,
    pub node_topics: HashSet<T>,
    pub node_topic_ids: HashSet<[u8; 32]>,
}

pub type Sender<M> = mpsc::Sender<M>;

pub type Receiver<M> = mpsc::Receiver<M>;

pub trait SubscriptionInfo<T> {
    type Error;

    fn subscribed_topics(&self) -> impl Future<Output = Result<Vec<T>, Self::Error>>;

    fn subscribed_topic_ids(&self) -> impl Future<Output = Result<Vec<[u8; 32]>, Self::Error>>;
}
