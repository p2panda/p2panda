// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};

use rand::Rng;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::DiscoveryStrategy;
use crate::address_book::{AddressBookStore, NodeInfo};

#[derive(Debug)]
pub struct RandomWalkConfig {
    pub bootstrap_mode_probability: f64,
}

impl Default for RandomWalkConfig {
    fn default() -> Self {
        Self {
            bootstrap_mode_probability: 0.02, // 2% chance
        }
    }
}

pub struct RandomWalk<R, S, T, ID, N> {
    my_id: ID,
    store: S,
    rng: Mutex<R>,
    bootstrap_mode: AtomicBool,
    config: RandomWalkConfig,
    _marker: PhantomData<(T, ID, N)>,
}

impl<R, S, T, ID, N> RandomWalk<R, S, T, ID, N>
where
    R: Rng,
    S: AddressBookStore<T, ID, N>,
{
    pub fn new(my_id: ID, store: S, rng: R) -> Self {
        Self::from_config(my_id, store, rng, RandomWalkConfig::default())
    }

    pub fn from_config(my_id: ID, store: S, rng: R, config: RandomWalkConfig) -> Self {
        Self {
            my_id,
            store,
            rng: Mutex::new(rng),
            bootstrap_mode: AtomicBool::new(true),
            config,
            _marker: PhantomData,
        }
    }
}

impl<R, S, T, ID, N> DiscoveryStrategy<N> for RandomWalk<R, S, T, ID, N>
where
    R: Rng,
    S: AddressBookStore<T, ID, N>,
    ID: Eq,
    N: NodeInfo<ID>,
{
    type Error = RandomWalkError<S, T, ID, N>;

    async fn next_node(&self) -> Result<Option<N>, Self::Error> {
        let bootstrap_mode = {
            if self.bootstrap_mode.load(Ordering::Relaxed) {
                true
            } else {
                // Flip a coin to see if we're switching into bootstrap mode.
                let coin = self
                    .rng
                    .lock()
                    .await
                    .random_bool(self.config.bootstrap_mode_probability);
                self.bootstrap_mode.store(true, Ordering::Relaxed);
                coin
            }
        };

        loop {
            let node_info = if bootstrap_mode {
                let result = self
                    .store
                    .random_bootstrap_node()
                    .await
                    .map_err(RandomWalkError::Store)?;

                // No bootstrap nodes available, try to pick any node instead and disable bootstrap
                // mode directly.
                if result.is_none() {
                    self.bootstrap_mode.store(false, Ordering::Relaxed);
                    self.store
                        .random_node()
                        .await
                        .map_err(RandomWalkError::Store)?
                } else {
                    result
                }
            } else {
                self.store
                    .random_node()
                    .await
                    .map_err(RandomWalkError::Store)?
            };

            let Some(node_info) = node_info else {
                return Ok(None);
            };

            if node_info.id() != self.my_id {
                return Ok(Some(node_info));
            }

            // Continue finding another random node if we picked ourselves or yield `None` if it is
            // the only item in the database (to not hang in this loop forever).
            let node_infos_len = self
                .store
                .all_node_infos_len()
                .await
                .map_err(RandomWalkError::Store)?;

            if node_infos_len == 1 {
                return Ok(None);
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum RandomWalkError<S, T, ID, N>
where
    S: AddressBookStore<T, ID, N>,
{
    #[error("{0}")]
    Store(S::Error),
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    use crate::address_book::AddressBookStore;
    use crate::test_utils::{TestInfo, TestStore};
    use crate::traits::DiscoveryStrategy;

    use super::RandomWalk;

    #[tokio::test]
    async fn never_yield_own_node_info() {
        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = TestStore::new(rng.clone());

        store
            .insert_node_info(TestInfo {
                id: 0,
                bootstrap: true,
                timestamp: 0,
            })
            .await
            .unwrap();

        let strategy = RandomWalk::new(0, store.clone(), rng);

        // This should never return a value and also not hang in an infinite loop if the only item
        // is ourselves in the database.
        assert!(strategy.next_node().await.unwrap().is_none());
    }
}
