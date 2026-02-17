// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::error::Error;
use std::time::Duration;

/// Node informations which can be stored in an address book, aiding discovery, sync, peer sampling
/// or other protocols.
///
/// Usually we want to separate node informations into a _local_ and _shareable_ part. Not all
/// information is meant to be shared with other nodes. NodeInfo is meant to be the _local_ or
/// private part while the associated `Transports` type is dedicated for _shareable_ or public
/// information.
pub trait NodeInfo<ID> {
    /// Information which usually holds addresses to establish connections for different transport
    /// protocols.
    ///
    /// This information is meant to be shared publicly on the network.
    type Transports;

    /// Returns node id for this information.
    fn id(&self) -> ID;

    /// Returns `true` if node is marked as a "boostrap".
    fn is_bootstrap(&self) -> bool;

    /// Returns `true` if node is marked as a "stale".
    ///
    /// Stale nodes should not be considered for connection attempts anymore and should not be
    /// shared during discovery with other nodes.
    fn is_stale(&self) -> bool;

    /// Returns attached transport information for this node, if available.
    fn transports(&self) -> Option<Self::Transports>;
}

/// Interface for storing, managing and querying information about nodes.
pub trait AddressBookStore<ID, N>
where
    N: NodeInfo<ID>,
{
    type Error: Error;

    /// Inserts information for a node.
    ///
    /// Returns `true` if entry got inserted or `false` if existing entry was updated.
    ///
    /// **Important:** Node information can be received from different (potentially untrusted)
    /// sources and can thus be outdated or invalid, this is why users of this store should check
    /// the timestamp and authenticity to only insert latest and valid data.
    fn insert_node_info(&self, info: N) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Removes information for a node.
    ///
    /// Returns `true` if entry was removed and `false` if it does not exist.
    fn remove_node_info(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;

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
    fn remove_older_than(
        &self,
        duration: Duration,
    ) -> impl Future<Output = Result<usize, Self::Error>>;

    /// Returns information about a node.
    ///
    /// Returns `None` if no information was found for this node.
    fn node_info(&self, id: &ID) -> impl Future<Output = Result<Option<N>, Self::Error>>;

    /// Returns topics of a node.
    fn node_topics(&self, id: &ID) -> impl Future<Output = Result<HashSet<[u8; 32]>, Self::Error>>;

    /// Returns a list of all known node informations.
    fn all_node_infos(&self) -> impl Future<Output = Result<Vec<N>, Self::Error>>;

    /// Returns the count of all known nodes.
    fn all_nodes_len(&self) -> impl Future<Output = Result<usize, Self::Error>>;

    /// Returns the count of all known bootstrap nodes.
    fn all_bootstrap_nodes_len(&self) -> impl Future<Output = Result<usize, Self::Error>>;

    /// Returns a list of node informations for a selected set.
    fn selected_node_infos(&self, ids: &[ID]) -> impl Future<Output = Result<Vec<N>, Self::Error>>;

    /// Sets the list of "topics" this node is "interested" in.
    ///
    /// Topics are usually shared privately and directly with nodes, this is why implementers
    /// usually want to simply overwrite the previous topic set (_not_ extend it).
    fn set_topics(
        &self,
        id: ID,
        topics: HashSet<[u8; 32]>,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Returns a list of informations about nodes which are all interested in at least one of the
    /// given topics in this set.
    fn node_infos_by_topics(
        &self,
        topics: &[[u8; 32]],
    ) -> impl Future<Output = Result<Vec<N>, Self::Error>>;

    /// Returns information from a randomly picked node or `None` when no information exists in the
    /// database.
    fn random_node(&self) -> impl Future<Output = Result<Option<N>, Self::Error>>;

    /// Returns information from a randomly picked "bootstrap" node or `None` when no information
    /// exists in the database.
    ///
    /// Nodes can be "marked" as bootstraps and discovery protocols can use that flag to prioritize
    /// them in their process.
    fn random_bootstrap_node(&self) -> impl Future<Output = Result<Option<N>, Self::Error>>;
}
