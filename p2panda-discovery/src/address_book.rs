// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::error::Error as StdError;
use std::pin::Pin;
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

    /// Returns `true` if node is marked as a "stale".
    ///
    /// Stale nodes should not be considered for connection attempts anymore and should not be
    /// shared during discovery with other nodes.
    fn is_stale(&self) -> bool;

    /// Returns attached transport information for this node, if available.
    fn transports(&self) -> Option<Self::Transports>;
}

pub trait AddressBookStore<ID, N> {
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

pub type BoxedError = Box<dyn StdError + Send + Sync + 'static>;

pub trait DynAddressBookStore<ID, N> {
    fn clone_box(&self) -> Box<dyn DynAddressBookStore<ID, N> + Send + 'static>;

    fn insert_node_info(
        &self,
        info: N,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>>;

    fn remove_node_info(
        &self,
        id: &ID,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>>;

    fn remove_older_than(
        &self,
        duration: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<usize, BoxedError>> + '_>>;

    fn node_info(
        &self,
        id: &ID,
    ) -> Pin<Box<dyn Future<Output = Result<Option<N>, BoxedError>> + '_>>;

    #[allow(clippy::type_complexity)]
    fn node_topics(
        &self,
        id: &ID,
    ) -> Pin<Box<dyn Future<Output = Result<HashSet<[u8; 32]>, BoxedError>> + '_>>;

    fn all_node_infos(&self) -> Pin<Box<dyn Future<Output = Result<Vec<N>, BoxedError>> + '_>>;

    fn all_nodes_len(&self) -> Pin<Box<dyn Future<Output = Result<usize, BoxedError>> + '_>>;

    fn all_bootstrap_nodes_len(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<usize, BoxedError>> + '_>>;

    fn selected_node_infos(
        &self,
        ids: &[ID],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<N>, BoxedError>> + '_>>;

    fn set_topics(
        &self,
        id: ID,
        topics: HashSet<[u8; 32]>,
    ) -> Pin<Box<dyn Future<Output = Result<(), BoxedError>> + '_>>;

    fn node_infos_by_topics(
        &self,
        topics: &[[u8; 32]],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<N>, BoxedError>> + '_>>;

    fn random_node(&self) -> Pin<Box<dyn Future<Output = Result<Option<N>, BoxedError>> + '_>>;

    fn random_bootstrap_node(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<N>, BoxedError>> + '_>>;
}

pub type BoxedAddressBookStore<ID, N> = Box<dyn DynAddressBookStore<ID, N> + Send + 'static>;

impl<ID, N, T> DynAddressBookStore<ID, N> for T
where
    ID: Clone + 'static,
    N: 'static,
    T: Clone + AddressBookStore<ID, N> + Send + 'static,
    T::Error: StdError + Send + Sync + 'static,
{
    fn clone_box(&self) -> BoxedAddressBookStore<ID, N> {
        Box::new(self.clone())
    }

    fn insert_node_info(
        &self,
        info: N,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>> {
        Box::pin(async move {
            self.insert_node_info(info)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn remove_node_info(
        &self,
        id: &ID,
    ) -> Pin<Box<dyn Future<Output = Result<bool, BoxedError>> + '_>> {
        let id = id.clone();
        Box::pin(async move {
            self.remove_node_info(&id)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn remove_older_than(
        &self,
        duration: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<usize, BoxedError>> + '_>> {
        Box::pin(async move {
            self.remove_older_than(duration)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn node_info(
        &self,
        id: &ID,
    ) -> Pin<Box<dyn Future<Output = Result<Option<N>, BoxedError>> + '_>> {
        let id = id.clone();
        Box::pin(async move {
            self.node_info(&id)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn node_topics(
        &self,
        id: &ID,
    ) -> Pin<Box<dyn Future<Output = Result<HashSet<[u8; 32]>, BoxedError>> + '_>> {
        let id = id.clone();
        Box::pin(async move {
            self.node_topics(&id)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn all_node_infos(&self) -> Pin<Box<dyn Future<Output = Result<Vec<N>, BoxedError>> + '_>> {
        Box::pin(async move {
            self.all_node_infos()
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn all_nodes_len(&self) -> Pin<Box<dyn Future<Output = Result<usize, BoxedError>> + '_>> {
        Box::pin(async move {
            self.all_nodes_len()
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn all_bootstrap_nodes_len(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<usize, BoxedError>> + '_>> {
        Box::pin(async move {
            self.all_bootstrap_nodes_len()
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn selected_node_infos(
        &self,
        ids: &[ID],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<N>, BoxedError>> + '_>> {
        let ids = ids.to_vec();
        Box::pin(async move {
            self.selected_node_infos(&ids)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn set_topics(
        &self,
        id: ID,
        topics: HashSet<[u8; 32]>,
    ) -> Pin<Box<dyn Future<Output = Result<(), BoxedError>> + '_>> {
        Box::pin(async move {
            self.set_topics(id, topics)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn node_infos_by_topics(
        &self,
        topics: &[[u8; 32]],
    ) -> Pin<Box<dyn Future<Output = Result<Vec<N>, BoxedError>> + '_>> {
        let topics = topics.to_vec();
        Box::pin(async move {
            self.node_infos_by_topics(&topics)
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn random_node(&self) -> Pin<Box<dyn Future<Output = Result<Option<N>, BoxedError>> + '_>> {
        Box::pin(async move {
            self.random_node()
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }

    fn random_bootstrap_node(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<N>, BoxedError>> + '_>> {
        Box::pin(async move {
            self.random_bootstrap_node()
                .await
                .map_err(|err| Box::new(err) as BoxedError)
        })
    }
}

pub struct WrappedAddressBookStore<ID, N>(BoxedAddressBookStore<ID, N>);

impl<ID, N> From<BoxedAddressBookStore<ID, N>> for WrappedAddressBookStore<ID, N> {
    fn from(value: BoxedAddressBookStore<ID, N>) -> Self {
        Self(value)
    }
}

impl<ID, N> AddressBookStore<ID, N> for WrappedAddressBookStore<ID, N> {
    type Error = BoxedError;

    async fn insert_node_info(&self, info: N) -> Result<bool, Self::Error> {
        self.0.as_ref().insert_node_info(info).await
    }

    async fn remove_node_info(&self, id: &ID) -> Result<bool, Self::Error> {
        self.0.as_ref().remove_node_info(id).await
    }

    async fn remove_older_than(&self, duration: Duration) -> Result<usize, Self::Error> {
        self.0.as_ref().remove_older_than(duration).await
    }

    async fn node_info(&self, id: &ID) -> Result<Option<N>, Self::Error> {
        self.0.as_ref().node_info(id).await
    }

    async fn node_topics(&self, id: &ID) -> Result<HashSet<[u8; 32]>, Self::Error> {
        self.0.as_ref().node_topics(id).await
    }

    async fn all_node_infos(&self) -> Result<Vec<N>, Self::Error> {
        self.0.as_ref().all_node_infos().await
    }

    async fn all_nodes_len(&self) -> Result<usize, Self::Error> {
        self.0.as_ref().all_nodes_len().await
    }

    async fn all_bootstrap_nodes_len(&self) -> Result<usize, Self::Error> {
        self.0.as_ref().all_bootstrap_nodes_len().await
    }

    async fn selected_node_infos(&self, ids: &[ID]) -> Result<Vec<N>, Self::Error> {
        self.0.as_ref().selected_node_infos(ids).await
    }

    async fn set_topics(&self, id: ID, topics: HashSet<[u8; 32]>) -> Result<(), Self::Error> {
        self.0.as_ref().set_topics(id, topics).await
    }

    async fn node_infos_by_topics(&self, topics: &[[u8; 32]]) -> Result<Vec<N>, Self::Error> {
        self.0.as_ref().node_infos_by_topics(topics).await
    }

    async fn random_node(&self) -> Result<Option<N>, Self::Error> {
        self.0.as_ref().random_node().await
    }

    async fn random_bootstrap_node(&self) -> Result<Option<N>, Self::Error> {
        self.0.as_ref().random_bootstrap_node().await
    }
}

impl<ID, N> std::fmt::Debug for WrappedAddressBookStore<ID, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WrappedAddressBookStore").finish()
    }
}

#[cfg(any(test, feature = "test_utils"))]
pub mod memory {
    use std::collections::{BTreeMap, HashSet};
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rand::Rng;
    use rand::seq::IteratorRandom;
    use tokio::sync::{Mutex, RwLock};

    use super::{AddressBookStore, NodeInfo};

    #[derive(Clone, Debug)]
    pub struct MemoryStore<R, ID, N> {
        rng: Arc<Mutex<R>>,
        node_infos: Arc<RwLock<BTreeMap<ID, N>>>,
        node_infos_last_changed: Arc<RwLock<BTreeMap<ID, u64>>>,
        topics: Arc<RwLock<BTreeMap<ID, HashSet<[u8; 32]>>>>,
    }

    impl<R, ID, N> MemoryStore<R, ID, N> {
        pub fn new(rng: R) -> Self {
            Self {
                rng: Arc::new(Mutex::new(rng)),
                node_infos: Arc::new(RwLock::new(BTreeMap::new())),
                node_infos_last_changed: Arc::new(RwLock::new(BTreeMap::new())),
                topics: Arc::new(RwLock::new(BTreeMap::new())),
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

    impl<R, ID, N> AddressBookStore<ID, N> for MemoryStore<R, ID, N>
    where
        R: Rng,
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

        async fn node_topics(&self, id: &ID) -> Result<HashSet<[u8; 32]>, Self::Error> {
            let topics = self.topics.read().await;
            let result = topics.get(id).cloned().unwrap_or(HashSet::new());
            Ok(result)
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
            Ok(node_infos
                .values()
                .filter(|info| !info.is_stale())
                .cloned()
                .collect())
        }

        async fn all_nodes_len(&self) -> Result<usize, Self::Error> {
            let node_infos = self.node_infos.read().await;
            Ok(node_infos.values().filter(|info| !info.is_stale()).count())
        }

        async fn all_bootstrap_nodes_len(&self) -> Result<usize, Self::Error> {
            let node_infos = self.node_infos.read().await;
            Ok(node_infos
                .values()
                .filter(|info| info.is_bootstrap() && !info.is_stale())
                .count())
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

        async fn set_topics(&self, id: ID, topics: HashSet<[u8; 32]>) -> Result<(), Self::Error> {
            let mut node_topics = self.topics.write().await;
            self.update_last_changed(id.clone()).await;
            node_topics.insert(id, HashSet::from_iter(topics.into_iter()));
            Ok(())
        }

        async fn node_infos_by_topics(&self, topics: &[[u8; 32]]) -> Result<Vec<N>, Self::Error> {
            let node_topics = self.topics.read().await;
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
            let node_infos = self.selected_node_infos(ids.as_slice()).await?;

            // Remove stale nodes.
            Ok(node_infos
                .into_iter()
                .filter(|info| !info.is_stale())
                .collect())
        }

        async fn random_node(&self) -> Result<Option<N>, Self::Error> {
            let node_infos = self.node_infos.read().await;
            let mut rng = self.rng.lock().await;
            let result = node_infos
                .values()
                .filter(|info| !info.is_stale())
                .choose(&mut *rng);
            Ok(result.cloned())
        }

        async fn random_bootstrap_node(&self) -> Result<Option<N>, Self::Error> {
            let node_infos = self.node_infos.read().await;
            let mut rng = self.rng.lock().await;
            let result = node_infos
                .values()
                .filter(|info| info.is_bootstrap() && !info.is_stale())
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
    use std::collections::HashSet;
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

        let cats = [100; 32];
        let dogs = [102; 32];
        let rain = [104; 32];
        let frogs = [106; 32];
        let trains = [200; 32];

        store.insert_node_info(TestInfo::new(1)).await.unwrap();
        store
            .set_topics(1, HashSet::from_iter([cats, dogs, rain]))
            .await
            .unwrap();

        store.insert_node_info(TestInfo::new(2)).await.unwrap();
        store
            .set_topics(2, HashSet::from_iter([rain]))
            .await
            .unwrap();

        store.insert_node_info(TestInfo::new(3)).await.unwrap();
        store
            .set_topics(3, HashSet::from_iter([dogs, frogs]))
            .await
            .unwrap();

        assert_eq!(
            store
                .node_infos_by_topics(&[dogs])
                .await
                .unwrap()
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<TestId>>(),
            vec![1, 3]
        );

        assert_eq!(
            store
                .node_infos_by_topics(&[frogs, rain])
                .await
                .unwrap()
                .into_iter()
                .map(|item| item.id)
                .collect::<Vec<TestId>>(),
            vec![1, 2, 3]
        );

        assert_eq!(
            store
                .node_infos_by_topics(&[trains])
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
        let mut rng = ChaCha20Rng::from_seed([1; 32]);
        let store = TestStore::new(rng.clone());

        for id in 0..100 {
            store
                .insert_node_info(TestInfo::new(id).with_random_address(&mut rng))
                .await
                .unwrap();
        }

        for id in 200..300 {
            store
                .insert_node_info(TestInfo::new_bootstrap(id).with_random_address(&mut rng))
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
