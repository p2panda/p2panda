// SPDX-License-Identifier: MIT OR Apache-2.0

use std::time::Duration;

/// Node informations which can be stored in an address book, aiding discovery, sync, peer sampling
/// or other protocols.
///
/// Usually we want to separate node informations into a _local_ and _shareable_ part. Not all
/// information is meant to be shared with other nodes.
pub trait NodeInfo<ID> {
    /// Information which usually holds addresses to establish connections for different transport
    /// protocols.
    type Transports;

    /// Returns node id for this information.
    fn id(&self) -> ID;

    /// Returns `true` if node is marked as a "boostrap".
    fn is_bootstrap(&self) -> bool;

    /// Returns attached transport information for this node, if available.
    fn transports(&self) -> Option<Self::Transports>;
}

pub trait AddressBookStore<T, ID, N> {
    type Error;

    /// Inserts information for a node.
    ///
    /// Returns `true` if entry got inserted or `false` if existing entry was updated.
    ///
    /// **Important:** Node information can be received from different (potentially untrusted)
    /// sources and can thus be outdated or invalid, this is why users of this store should check
    /// the timestamp and authenticity to only insert latest and valid data.
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

    /// Returns a list of all known node informations.
    fn all_node_infos(&self) -> impl Future<Output = Result<Vec<N>, Self::Error>>;

    /// Returns the count of all known node informations.
    fn all_node_infos_len(&self) -> impl Future<Output = Result<usize, Self::Error>>;

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

#[cfg(any(test, feature = "test_utils"))]
pub mod memory {
    use std::collections::{BTreeMap, HashSet};
    use std::convert::Infallible;
    use std::hash::Hash as StdHash;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rand::Rng;
    use rand::seq::IteratorRandom;
    use tokio::sync::{Mutex, RwLock};

    use super::{AddressBookStore, NodeInfo};

    #[derive(Clone, Debug)]
    pub struct MemoryStore<R, T, ID, N> {
        rng: Arc<Mutex<R>>,
        node_infos: Arc<RwLock<BTreeMap<ID, N>>>,
        node_infos_last_changed: Arc<RwLock<BTreeMap<ID, u64>>>,
        node_topics: Arc<RwLock<BTreeMap<ID, HashSet<T>>>>,
        node_topic_ids: Arc<RwLock<BTreeMap<ID, HashSet<[u8; 32]>>>>,
    }

    impl<R, T, ID, N> MemoryStore<R, T, ID, N> {
        pub fn new(rng: R) -> Self {
            Self {
                rng: Arc::new(Mutex::new(rng)),
                node_infos: Arc::new(RwLock::new(BTreeMap::new())),
                node_infos_last_changed: Arc::new(RwLock::new(BTreeMap::new())),
                node_topics: Arc::new(RwLock::new(BTreeMap::new())),
                node_topic_ids: Arc::new(RwLock::new(BTreeMap::new())),
            }
        }

        /// Updates the "last changed" timestamp for a node.
        ///
        /// Remember at what time this entry got changed, this helps us later to do garbage
        /// collection of "old" entries.
        async fn update_last_changed(&self, id: ID)
        where
            ID: Ord,
        {
            let mut node_infos_last_changed = self.node_infos_last_changed.write().await;
            node_infos_last_changed.insert(id, current_timestamp());
        }

        #[cfg(test)]
        pub async fn set_last_changed(&self, id: ID, timestamp: u64)
        where
            ID: Ord,
        {
            let mut node_infos_last_changed = self.node_infos_last_changed.write().await;
            node_infos_last_changed.insert(id, timestamp);
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
            let mut node_infos = self.node_infos.write().await;
            self.update_last_changed(info.id()).await;
            Ok(node_infos.insert(info.id(), info).is_none())
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

        async fn all_node_infos_len(&self) -> Result<usize, Self::Error> {
            let node_infos = self.node_infos.read().await;
            Ok(node_infos.len())
        }

        async fn remove_node_info(&self, id: &ID) -> Result<bool, Self::Error> {
            let mut node_infos = self.node_infos.write().await;
            Ok(node_infos.remove(id).is_some())
        }

        async fn remove_older_than(&self, duration: Duration) -> Result<usize, Self::Error> {
            let mut counter: usize = 0;
            let mut node_infos = self.node_infos.write().await;
            let infos_last_changed = self.node_infos_last_changed.read().await;
            node_infos.retain(|id, _| {
                let last_changed = infos_last_changed
                    .get(id)
                    .cloned()
                    .expect("last_changed is always set when we write to store");
                let keep = last_changed > current_timestamp() - duration.as_secs();
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
            self.update_last_changed(id.clone()).await;
            node_topics.insert(id, HashSet::from_iter(topics.into_iter()));
            Ok(())
        }

        async fn set_topic_ids(
            &self,
            id: ID,
            topic_ids: impl IntoIterator<Item = [u8; 32]>,
        ) -> Result<(), Self::Error> {
            let mut node_topic_ids = self.node_topic_ids.write().await;
            self.update_last_changed(id.clone()).await;
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

        async fn node_infos_by_topic_ids(
            &self,
            topic_ids: &[[u8; 32]],
        ) -> Result<Vec<N>, Self::Error> {
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

    pub(crate) fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is not behind")
            .as_secs()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand_chacha::ChaCha20Rng;
    use rand_chacha::rand_core::SeedableRng;

    use crate::test_utils::{TestId, TestInfo, TestStore};

    use super::memory::current_timestamp;
    use super::{AddressBookStore, NodeInfo};

    #[tokio::test]
    async fn insert_node_info() {
        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = TestStore::new(rng);

        let node_info_1 = TestInfo::new(1);

        let result = store.insert_node_info(node_info_1.clone()).await.unwrap();

        assert!(result);
        assert_eq!(
            store.node_info(&node_info_1.id).await.unwrap(),
            Some(node_info_1.clone())
        );
    }

    #[tokio::test]
    async fn set_and_query_topics() {
        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = TestStore::new(rng);

        store.insert_node_info(TestInfo::new(1)).await.unwrap();
        store
            .set_topics(1, ["cats".into(), "dogs".into(), "rain".into()])
            .await
            .unwrap();

        store.insert_node_info(TestInfo::new(2)).await.unwrap();
        store.set_topics(2, ["rain".into()]).await.unwrap();

        store.insert_node_info(TestInfo::new(3)).await.unwrap();
        store
            .set_topics(3, ["dogs".into(), "frogs".into()])
            .await
            .unwrap();

        assert_eq!(
            store
                .node_infos_by_topics(&["dogs".into()])
                .await
                .unwrap()
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<TestId>>(),
            vec![1, 3]
        );

        assert_eq!(
            store
                .node_infos_by_topics(&["frogs".into(), "rain".into()])
                .await
                .unwrap()
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<TestId>>(),
            vec![1, 2, 3]
        );

        assert_eq!(
            store
                .node_infos_by_topics(&["trains".into()])
                .await
                .unwrap()
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<TestId>>(),
            vec![]
        );
    }

    #[tokio::test]
    async fn remove_outdated_node_infos() {
        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = TestStore::new(rng);

        store.insert_node_info(TestInfo::new(1)).await.unwrap();
        store
            .set_last_changed(1, current_timestamp() - (60 * 2))
            .await; // 2 minutes "old"

        // Timestamp of this entry will be set to "now" automatically.
        store.insert_node_info(TestInfo::new(2)).await.unwrap();

        // Expect removing one item from database.
        let result = store
            .remove_older_than(Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(result, 1);
        assert!(store.node_info(&1).await.unwrap().is_none());
        assert!(store.node_info(&2).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn sample_random_nodes() {
        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = TestStore::new(rng);

        for id in 0..100 {
            store.insert_node_info(TestInfo::new(id)).await.unwrap();
        }

        for id in 200..300 {
            store
                .insert_node_info(TestInfo::new_bootstrap(id))
                .await
                .unwrap();
        }

        // Sampling random nodes should give us some variety.
        for _ in 0..100 {
            assert_ne!(
                store.random_node().await.unwrap().unwrap(),
                store.random_node().await.unwrap().unwrap(),
            );
        }

        for _ in 0..100 {
            let sample_1 = store.random_bootstrap_node().await.unwrap().unwrap();
            let sample_2 = store.random_bootstrap_node().await.unwrap().unwrap();
            assert_ne!(sample_1, sample_2,);
            assert!(sample_1.is_bootstrap());
            assert!(sample_2.is_bootstrap());
        }
    }
}
