// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;

use futures_util::{Sink, Stream};

use crate::address_book::NodeInfo;

/// Peer-Sampling Strategy used for discovery.
pub trait DiscoveryStrategy<T, ID, N>
where
    N: NodeInfo<ID>,
{
    type Error;

    fn next_node(
        &self,
        previous: Option<&DiscoveryResult<T, ID, N>>,
    ) -> impl Future<Output = Result<Option<ID>, Self::Error>>;
}

/// Protocol between two parties Alice and Bob to exchange node informations where Alice
/// "initiated" the protocol and Bob "accepted" it.
///
/// Ideally (when nothing went wrong) both parties end up with a `DiscoveryResult` which contains
/// the information they learned about during this exchange.
pub trait DiscoveryProtocol<T, ID, N>
where
    N: NodeInfo<ID>,
{
    type Error;

    type Message;

    fn alice(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> impl Future<Output = Result<DiscoveryResult<T, ID, N>, Self::Error>>;

    fn bob(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> impl Future<Output = Result<DiscoveryResult<T, ID, N>, Self::Error>>;
}

/// Result containing node information and topics of a session between Alice and Bob running a
/// discovery protocol.
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

impl<T, ID, N> DiscoveryResult<T, ID, N>
where
    N: NodeInfo<ID>,
{
    pub fn new(remote_node_id: ID) -> Self {
        Self {
            remote_node_id,
            node_transport_infos: BTreeMap::new(),
            node_topics: HashSet::new(),
            node_topic_ids: HashSet::new(),
        }
    }
}

/// Interface required by discovery protocols to learn which topics (eventual consistent sync) and
/// topic ids (ephemeral messaging) the own node is currently interested in.
pub trait SubscriptionInfo<T> {
    type Error;

    fn subscribed_topics(&self) -> impl Future<Output = Result<Vec<T>, Self::Error>>;

    fn subscribed_topic_ids(&self) -> impl Future<Output = Result<Vec<[u8; 32]>, Self::Error>>;
}
