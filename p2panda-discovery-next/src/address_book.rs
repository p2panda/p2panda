// SPDX-License-Identifier: MIT OR Apache-2.0

// @TODO: Move this into `p2panda-store` when it's ready.
use std::collections::{BTreeMap, HashSet};
use std::convert::Infallible;
use std::error::Error;
use std::hash::Hash as StdHash;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::Rng;
use rand::seq::IteratorRandom;
use tokio::sync::{Mutex, RwLock};

pub trait NodeInfo<ID> {
    /// Returns node id for this information.
    fn id(&self) -> ID;

    /// Returns `true` if node is marked as a "boostrap".
    fn is_bootstrap(&self) -> bool;

    /// Returns UNIX timestamp of this information.
    fn timestamp(&self) -> u64;
}

pub trait AddressBookStore<T, ID, N> {
    type Error: Error;

    /// Inserts new information for a node if it is newer than the previous one.
    ///
    /// Returns `true` if entry got inserted or `false` if insertion got ignored since given
    /// information is too old.
    ///
    /// **Important:** Node information can be received from different (potentially untrusted)
    /// sources and can thus be outdated or invalid, this is why implementers of this store should
    /// check the timestamp and authenticity to only insert latest and valid data.
    fn insert_node_info(&self, info: N) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Removes information for a node. Returns `true` if entry was removed and `false` if it does
    /// not exist.
    fn remove_node_info(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Remove all node informations which are older than the given duration (from now). Returns
    /// number of removed entries.
    ///
    /// Applications should frequently clean up "old" information about nodes to remove potentially
    /// "useless" data from the network and not unnecessarily share sensitive information, even
    /// when outdated. This method has a similar function as a TTL (Time-To-Life) record but is
    /// less authoritative.
    fn remove_older_than(
        &self,
        duration: Duration,
    ) -> impl Future<Output = Result<usize, Self::Error>>;

    /// Returns information about a node.
    ///
    /// Returns `None` if no information was found for this node.
    fn node_info(&self, id: &ID) -> impl Future<Output = Result<Option<N>, Self::Error>>;

    /// Returns a list of all known node informations.
    fn all_node_infos(&self) -> impl Future<Output = Result<Vec<N>, Self::Error>>;

    /// Returns a list of node informations for a selected set.
    fn selected_node_infos(&self, ids: &[ID]) -> impl Future<Output = Result<Vec<N>, Self::Error>>;

    /// Sets the list of "topics" this node is "interested" in.
    ///
    /// Topics are usually shared privately and directly with nodes, this is why implementers
    /// usually want to simply overwrite the previous topic set (_not_ extend it).
    fn set_topics(
        &self,
        id: ID,
        topics: impl IntoIterator<Item = T>,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Sets the list of "topic identifiers" this node is "interested" in in
    ///
    /// Topic ids for gossip overlays (used for ephemeral messaging) are usually shared privately
    /// and directly with nodes, this is why implementers usually want to simply overwrite the
    /// previous topic id set (_not_ extend it).
    fn set_topic_ids(
        &self,
        id: ID,
        topic_ids: impl IntoIterator<Item = [u8; 32]>,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Returns a list of informations about nodes which are all interested in at least one of the
    /// given topics in this set.
    fn node_infos_by_topics(
        &self,
        topics: &[T],
    ) -> impl Future<Output = Result<Vec<N>, Self::Error>>;

    /// Returns a list of informations about nodes which are all interested in at least one of the
    /// given topic ids in this set.
    fn node_infos_by_topic_ids(
        &self,
        topic_ids: &[[u8; 32]],
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

#[derive(Clone, Debug)]
pub struct MemoryStore<R, T, ID, N> {
    rng: Arc<Mutex<R>>,
    node_infos: Arc<RwLock<BTreeMap<ID, N>>>,
    node_topics: Arc<RwLock<BTreeMap<ID, HashSet<T>>>>,
    node_topic_ids: Arc<RwLock<BTreeMap<ID, HashSet<[u8; 32]>>>>,
}

impl<R, T, ID, N> MemoryStore<R, T, ID, N> {
    pub fn new(rng: R) -> Self {
        Self {
            rng: Arc::new(Mutex::new(rng)),
            node_infos: Arc::new(RwLock::new(BTreeMap::new())),
            node_topics: Arc::new(RwLock::new(BTreeMap::new())),
            node_topic_ids: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

impl<R, T, ID, N> AddressBookStore<T, ID, N> for MemoryStore<R, T, ID, N>
where
    R: Rng,
    T: Eq + StdHash,
    ID: Clone + Ord,
    N: Clone + NodeInfo<ID>,
{
    type Error = Infallible;

    async fn insert_node_info(&self, info: N) -> Result<bool, Self::Error> {
        let is_newer = {
            match self.node_info(&info.id()).await? {
                Some(current) => info.timestamp() > current.timestamp(),
                None => true,
            }
        };
        if !is_newer {
            return Ok(false);
        }

        let mut node_infos = self.node_infos.write().await;
        node_infos.insert(info.id(), info);
        Ok(true)
    }

    async fn node_info(&self, id: &ID) -> Result<Option<N>, Self::Error> {
        let node_infos = self.node_infos.read().await;
        Ok(node_infos.get(id).cloned())
    }

    async fn selected_node_infos(&self, ids: &[ID]) -> Result<Vec<N>, Self::Error> {
        let node_infos = self.node_infos.read().await;
        let result = node_infos
            .iter()
            .filter_map(
                |(id, info)| {
                    if ids.contains(id) { Some(info) } else { None }
                },
            )
            .cloned()
            .collect();
        Ok(result)
    }

    async fn all_node_infos(&self) -> Result<Vec<N>, Self::Error> {
        let node_infos = self.node_infos.read().await;
        Ok(node_infos.values().cloned().collect())
    }

    async fn remove_node_info(&self, id: &ID) -> Result<bool, Self::Error> {
        let mut node_infos = self.node_infos.write().await;
        Ok(node_infos.remove(id).is_some())
    }

    async fn remove_older_than(&self, duration: Duration) -> Result<usize, Self::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is not behind");
        let mut counter: usize = 0;
        let mut node_infos = self.node_infos.write().await;
        node_infos.retain(|_id, info| {
            let keep = Duration::from_secs(info.timestamp()) > now + duration;
            if !keep {
                counter += 1;
            }
            keep
        });
        Ok(counter)
    }

    async fn set_topics(
        &self,
        id: ID,
        topics: impl IntoIterator<Item = T>,
    ) -> Result<(), Self::Error> {
        let mut node_topics = self.node_topics.write().await;
        node_topics.insert(id, HashSet::from_iter(topics.into_iter()));
        Ok(())
    }

    async fn set_topic_ids(
        &self,
        id: ID,
        topic_ids: impl IntoIterator<Item = [u8; 32]>,
    ) -> Result<(), Self::Error> {
        let mut node_topic_ids = self.node_topic_ids.write().await;
        node_topic_ids.insert(id, HashSet::from_iter(topic_ids.into_iter()));
        Ok(())
    }

    async fn node_infos_by_topics(&self, topics: &[T]) -> Result<Vec<N>, Self::Error> {
        let node_topics = self.node_topics.read().await;
        let ids: Vec<ID> = node_topics
            .iter()
            .filter_map(|(node_id, node_topics)| {
                if node_topics.iter().any(|t| topics.contains(t)) {
                    Some(node_id.clone())
                } else {
                    None
                }
            })
            .collect();
        self.selected_node_infos(ids.as_slice()).await
    }

    async fn node_infos_by_topic_ids(&self, topic_ids: &[[u8; 32]]) -> Result<Vec<N>, Self::Error> {
        let node_topic_ids = self.node_topic_ids.read().await;
        let ids: Vec<ID> = node_topic_ids
            .iter()
            .filter_map(|(node_id, node_topic_ids)| {
                if node_topic_ids.iter().any(|t| topic_ids.contains(t)) {
                    Some(node_id.clone())
                } else {
                    None
                }
            })
            .collect();
        self.selected_node_infos(ids.as_slice()).await
    }

    async fn random_node(&self) -> Result<Option<N>, Self::Error> {
        let node_infos = self.node_infos.read().await;
        let mut rng = self.rng.lock().await;
        let result = node_infos.values().choose(&mut *rng);
        Ok(result.cloned())
    }

    async fn random_bootstrap_node(&self) -> Result<Option<N>, Self::Error> {
        let node_infos = self.node_infos.read().await;
        let mut rng = self.rng.lock().await;
        let result = node_infos
            .values()
            .filter(|info| info.is_bootstrap())
            .choose(&mut *rng);
        Ok(result.cloned())
    }
}

#[cfg(test)]
mod tests {
    use rand_chacha::ChaCha20Rng;
    use rand_chacha::rand_core::SeedableRng;

    use super::{AddressBookStore, MemoryStore, NodeInfo};

    type TestId = usize;

    type TestTopic = &'static str;

    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct TestInfo {
        id: TestId,
        bootstrap: bool,
        timestamp: u64,
        address: String,
    }

    impl NodeInfo<TestId> for TestInfo {
        fn id(&self) -> TestId {
            self.id
        }

        fn is_bootstrap(&self) -> bool {
            self.bootstrap
        }

        fn timestamp(&self) -> u64 {
            self.timestamp
        }
    }

    type TestStore<R> = MemoryStore<R, TestTopic, TestId, TestInfo>;

    #[tokio::test]
    async fn insert_newer_node_info() {
        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = TestStore::new(rng);

        let node_info_1 = TestInfo {
            id: 1,
            bootstrap: false,
            timestamp: 1234,
            address: "192.168.0.100".into(),
        };

        // Correctly inserts and gets node info.
        let result = store.insert_node_info(node_info_1.clone()).await.unwrap();

        assert!(result);
        assert_eq!(
            store.node_info(&node_info_1.id).await.unwrap(),
            Some(node_info_1.clone())
        );

        // Ignores outdated node infos on insertion.
        let mut node_info_3 = node_info_1.clone();
        node_info_3.timestamp -= 1;

        let result = store.insert_node_info(node_info_3.clone()).await.unwrap();

        assert!(!result);
        assert_eq!(
            store.node_info(&node_info_3.id).await.unwrap(),
            Some(node_info_1)
        );
    }
}
