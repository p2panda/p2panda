// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};

use rand::Rng;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::DiscoveryStrategy;
use crate::address_book::AddressBookStore;

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
    pub fn new(store: S, rng: R) -> Self {
        Self::from_config(store, rng, RandomWalkConfig::default())
    }

    pub fn from_config(store: S, rng: R, config: RandomWalkConfig) -> Self {
        Self {
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

        Ok(node_info)
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
