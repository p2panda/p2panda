// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_store::address_book::{AddressBookStore, NodeInfo};
use rand::{Rng, RngExt, seq::IteratorRandom};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use crate::{DiscoveryResult, DiscoveryStrategy};

#[derive(Debug)]
pub struct RandomWalkerConfig {
    /// Probability of resetting the random walk and starting from scratch, determined on every
    /// walking step.
    ///
    /// ```text
    /// 0.0 = Never reset
    /// 1.0 = Always reset
    /// ```
    ///
    /// Defaults to 0.02 (2%) probability.
    pub reset_walk_probability: f64,
}

impl Default for RandomWalkerConfig {
    fn default() -> Self {
        Self {
            reset_walk_probability: 0.02, // 2% chance
        }
    }
}

/// Breadth-first "random walk" peer sampling strategy.
///
/// Strategy to traverse a network of nodes of possibly unknown size and shape combined with a
/// discovery protocol to exchange known nodes.
///
/// A random next node is selected to initiate the discovery protocol, starting from a locally
/// configured set of "bootstrap nodes" if available. The "walker" continues with another random
/// node sampled from a set of unvisited nodes - and repeats.
///
/// The algorithm resets itself and starts from scratch - based on a configurable probability or
/// when no new nodes could be found anymore. See `RandomWalkConfig` for details.
pub struct RandomWalker<R, S, ID, N> {
    my_id: ID,
    store: S,
    rng: Mutex<R>,
    config: RandomWalkerConfig,
    state: RwLock<RandomWalkerState<ID>>,
    _marker: PhantomData<N>,
}

struct RandomWalkerState<ID> {
    unvisited: HashSet<ID>,
    visited: HashSet<ID>,
}

impl<ID> Default for RandomWalkerState<ID> {
    fn default() -> Self {
        Self {
            unvisited: HashSet::new(),
            visited: HashSet::new(),
        }
    }
}

impl<R, S, ID, N> RandomWalker<R, S, ID, N>
where
    R: Rng,
    S: AddressBookStore<ID, N>,
    ID: Clone + Eq + StdHash,
    N: NodeInfo<ID>,
{
    pub fn new(my_id: ID, store: S, rng: R) -> Self {
        Self::from_config(my_id, store, rng, RandomWalkerConfig::default())
    }

    pub fn from_config(my_id: ID, store: S, rng: R, config: RandomWalkerConfig) -> Self {
        Self {
            my_id,
            store,
            rng: Mutex::new(rng),
            state: RwLock::new(RandomWalkerState::default()),
            config,
            _marker: PhantomData,
        }
    }

    /// Load all currently known nodes from address book and mark them as "unvisisted", essentially
    /// resetting walker to original state.
    async fn reset(&self) -> Result<(), RandomWalkError<S, ID, N>> {
        // If we're running multiple random walkers parallely we might receive node information
        // here we haven't found ourselves since the store is shared among all walkers.
        let all_nodes = self
            .store
            .all_node_infos()
            .await
            .map_err(RandomWalkError::Store)?
            .into_iter()
            .filter_map(|info| {
                // Remove ourselves.
                let id = info.id();
                if id != self.my_id { Some(id) } else { None }
            });
        {
            let mut state = self.state.write().await;
            state.unvisited.extend(all_nodes);
            // Mark ourselves as "visited".
            state.visited = HashSet::from([self.my_id.clone()]);
        }
        Ok(())
    }

    /// Select a random bootstrap node if available, fall back to any random node otherwise.
    async fn random_bootstrap_node(&self) -> Result<Option<ID>, RandomWalkError<S, ID, N>> {
        let bootstrap_node = loop {
            let node_id = self
                .store
                .random_bootstrap_node()
                .await
                .map_err(RandomWalkError::Store)?
                .map(|info| info.id());

            let Some(node_id) = node_id else {
                break None;
            };

            if node_id != self.my_id {
                break Some(node_id);
            }

            // Safeguard: Avoid returning ourselves if we've been accidentially configured as a
            // "boostrap" node.
            //
            // Continue finding another random node if we picked ourselves or yield `None` if it is
            // the only item in the database (to not hang in this loop forever).
            let bootstrap_nodes_len = self
                .store
                .all_bootstrap_nodes_len()
                .await
                .map_err(RandomWalkError::Store)?;

            // The store de-duplicates entries, we can be sure we will only ever be once in the
            // database.
            if bootstrap_nodes_len == 1 {
                return Ok(None);
            }
        };

        // No bootstrap nodes available, try to pick any node instead.
        if bootstrap_node.is_none() {
            self.random_unvisited_node().await
        } else {
            Ok(bootstrap_node)
        }
    }

    /// Select next random node for the walk.
    ///
    /// Samples a random node from the set of unvisited nodes.
    async fn random_unvisited_node(&self) -> Result<Option<ID>, RandomWalkError<S, ID, N>> {
        let state = self.state.read().await;
        let mut rng = self.rng.lock().await;
        let sampled = state.unvisited.iter().choose(&mut rng);
        Ok(sampled.cloned())
    }

    /// Merge all unvisited nodes into our set from previous discovery exchange.
    async fn merge_previous(&self, previous: &DiscoveryResult<ID, N>) {
        let node_ids = previous.transport_infos.keys();
        let mut state = self.state.write().await;
        for id in node_ids {
            if !state.visited.contains(id) {
                state.unvisited.insert(id.clone());
            }
        }
    }

    /// Mark node as visited.
    async fn mark_visited(&self, id: &ID) {
        let mut state = self.state.write().await;
        state.visited.insert(id.clone());
        state.unvisited.remove(id);
    }
}

impl<R, S, ID, N> DiscoveryStrategy<ID, N> for RandomWalker<R, S, ID, N>
where
    R: Rng,
    S: AddressBookStore<ID, N>,
    ID: Clone + Eq + StdHash,
    N: NodeInfo<ID>,
{
    type Error = RandomWalkError<S, ID, N>;

    async fn next_node(
        &self,
        previous: Option<&DiscoveryResult<ID, N>>,
    ) -> Result<Option<ID>, Self::Error> {
        if let Some(previous) = previous {
            self.merge_previous(previous).await;
        }

        let reset = {
            if previous.is_none() {
                // Always reset at the beginning to initialise everything and start with
                // "bootstrap" nodes.
                true
            } else if self.state.read().await.unvisited.is_empty() {
                // We've visited all nodes.
                true
            } else {
                // Flip a coin.
                self.rng
                    .lock()
                    .await
                    .random_bool(self.config.reset_walk_probability)
            }
        };

        let node_id = if reset {
            self.reset().await?;
            self.random_bootstrap_node().await?
        } else {
            self.random_unvisited_node().await?
        };

        if let Some(ref node_id) = node_id {
            self.mark_visited(node_id).await;
        }

        Ok(node_id)
    }
}

#[derive(Debug, Error)]
pub enum RandomWalkError<S, ID, N>
where
    S: AddressBookStore<ID, N>,
    N: NodeInfo<ID>,
{
    #[error("{0}")]
    Store(S::Error),
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use p2panda_core::PrivateKey;
    use p2panda_store::address_book::AddressBookStore;
    use p2panda_store::address_book::test_utils::{TestNodeId, TestNodeInfo};
    use p2panda_store::{SqliteStore, tx_unwrap};
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    use crate::traits::{DiscoveryResult, DiscoveryStrategy};

    use super::{RandomWalker, RandomWalkerConfig};

    #[tokio::test]
    async fn explore_full_graph() {
        let node_ids = {
            let mut map = Vec::with_capacity(9);
            for _ in 0..9 {
                map.push(PrivateKey::new().public_key());
            }
            map
        };

        // Initially node 0 only knows 1 (bootstrap) and explores the rest of the graph through
        // it by traversing their (transitive) neighbors.
        //
        //    0
        //    | (bootstrap)
        //    1
        //   / \
        //  2   3
        //  |   |
        //  4   5
        //     /|\
        //    6 7-8
        //
        let graph = HashMap::from([
            (node_ids[0], vec![]),
            (node_ids[1], vec![node_ids[2], node_ids[3]]),
            (node_ids[2], vec![node_ids[4]]),
            (node_ids[3], vec![node_ids[5]]),
            (node_ids[4], vec![]),
            (node_ids[5], vec![node_ids[6], node_ids[7], node_ids[8]]),
            (node_ids[6], vec![]),
            (node_ids[7], vec![node_ids[8]]),
            (node_ids[8], vec![]),
        ]);

        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = SqliteStore::temporary().await;

        tx_unwrap!(store, {
            store
                .insert_node_info(TestNodeInfo::new(node_ids[0]))
                .await
                .unwrap();
            store
                .insert_node_info(TestNodeInfo::new_bootstrap(node_ids[1]))
                .await
                .unwrap();
        });

        let strategy = RandomWalker::new(node_ids[0], store.clone(), rng);

        let mut visited: HashSet<TestNodeId> = HashSet::new();
        let mut previous: Option<DiscoveryResult<TestNodeId, TestNodeInfo>> = None;

        for _ in 0..graph.len() - 1 {
            let id = strategy
                .next_node(previous.as_ref())
                .await
                .unwrap()
                .expect("should return a Some value");

            visited.insert(id);
            previous = Some(DiscoveryResult::from_neighbors(id, graph.get(&id).unwrap()));
        }

        // Traversal visited all nodes in the network.
        assert_eq!(visited.len(), graph.len() - 1);
    }

    #[tokio::test]
    async fn mark_nodes_as_visited() {
        // This test checks if the random walker is correctly marking nodes as "visited".
        const NUM_NODES: usize = 32;

        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = SqliteStore::temporary().await;

        let node_ids = {
            let mut map = Vec::with_capacity(NUM_NODES);
            for _ in 0..NUM_NODES {
                map.push(PrivateKey::new().public_key());
            }
            map
        };

        tx_unwrap!(store, {
            for idx in 0..NUM_NODES {
                let node_id = node_ids[idx];
                store
                    .insert_node_info(TestNodeInfo::new(node_id))
                    .await
                    .unwrap();
            }
        });

        let strategy = RandomWalker::from_config(
            node_ids[0],
            store.clone(),
            rng,
            RandomWalkerConfig {
                // Never reset in this test.
                reset_walk_probability: 0.0,
            },
        );

        let mut visited: HashSet<TestNodeId> = HashSet::new();
        let mut previous: Option<DiscoveryResult<TestNodeId, TestNodeInfo>> = None;

        for _ in 0..NUM_NODES - 1 {
            let id = strategy
                .next_node(previous.as_ref())
                .await
                .unwrap()
                .expect("should return a Some value");

            if !visited.insert(id) {
                panic!("should never return duplicates");
            }

            previous = Some(DiscoveryResult::new(id));
        }

        // Next iteration we visited all possible nodes, the walker should automatically reset and
        // start re-visiting nodes again.
        let id = strategy
            .next_node(previous.as_ref())
            .await
            .unwrap()
            .expect("should return a Some value");
        assert!(visited.contains(&id));
    }

    #[tokio::test]
    async fn never_yield_own_node_info() {
        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = SqliteStore::temporary().await;

        let node_id = PrivateKey::new().public_key();

        tx_unwrap!(store, {
            store
                .insert_node_info(TestNodeInfo::new_bootstrap(node_id))
                .await
                .unwrap();
        });

        let strategy = RandomWalker::<_, _, _, TestNodeInfo>::new(node_id, store.clone(), rng);

        // This should never return a value and also not hang in an infinite loop if the only item
        // is ourselves in the database.
        assert!(strategy.next_node(None).await.unwrap().is_none());
    }
}
