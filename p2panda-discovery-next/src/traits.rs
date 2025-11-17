// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::fmt::Debug;

use futures_util::{Sink, Stream};

use crate::address_book::NodeInfo;

/// Peer-Sampling Strategy used for discovery.
pub trait DiscoveryStrategy<ID, N>
where
    N: NodeInfo<ID>,
{
    type Error;

    fn next_node(
        &self,
        previous: Option<&DiscoveryResult<ID, N>>,
    ) -> impl Future<Output = Result<Option<ID>, Self::Error>>;
}

/// Protocol between two parties Alice and Bob to exchange node informations where Alice
/// "initiated" the protocol and Bob "accepted" it.
///
/// Ideally (when nothing went wrong) both parties end up with a `DiscoveryResult` which contains
/// the information they learned about during this exchange.
pub trait DiscoveryProtocol<ID, N>
where
    N: NodeInfo<ID>,
{
    type Error;

    type Message;

    fn alice(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> impl Future<Output = Result<DiscoveryResult<ID, N>, Self::Error>>;

    fn bob(
        &self,
        tx: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        rx: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> impl Future<Output = Result<DiscoveryResult<ID, N>, Self::Error>>;
}

/// Result containing node information and topics of a session between Alice and Bob running a
/// discovery protocol.
#[derive(Clone, Debug)]
pub struct DiscoveryResult<ID, N>
where
    N: NodeInfo<ID>,
{
    /// Identifier of the node where we got these results from.
    pub remote_node_id: ID,

    /// Transport information we've learned from this node, potentially also transitive information
    /// about other nodes as well.
    pub node_transport_infos: BTreeMap<ID, N::Transports>,

    /// Sync "topics" this node is currently interested in.
    pub sync_topics: HashSet<[u8; 32]>,

    /// Epehemeral messaging "topics" this node is currently interested in.
    pub ephemeral_messaging_topics: HashSet<[u8; 32]>,
}

impl<ID, N> DiscoveryResult<ID, N>
where
    N: NodeInfo<ID>,
{
    pub fn new(remote_node_id: ID) -> Self {
        Self {
            remote_node_id,
            node_transport_infos: BTreeMap::new(),
            sync_topics: HashSet::new(),
            ephemeral_messaging_topics: HashSet::new(),
        }
    }
}

/// Interface required by discovery protocols to learn which topics for eventual consistent sync
/// and ephemeral messaging the local node is currently interested in.
pub trait LocalTopics {
    type Error;

    fn sync_topics(&self) -> impl Future<Output = Result<HashSet<[u8; 32]>, Self::Error>>;

    fn ephemeral_messaging_topics(
        &self,
    ) -> impl Future<Output = Result<HashSet<[u8; 32]>, Self::Error>>;
}
