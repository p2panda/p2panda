// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Mutex;

use rand_chacha::rand_core::{SeedableRng, TryRngCore};
use thiserror::Error;

/// Cryptographically-secure random number generator that uses the ChaCha algorithm.
#[derive(Debug)]
pub struct Rng {
    rng: Mutex<rand_chacha::ChaCha20Rng>,
}

impl Default for Rng {
    fn default() -> Self {
        Self {
            rng: Mutex::new(rand_chacha::ChaCha20Rng::from_os_rng()),
        }
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl Rng {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            rng: Mutex::new(rand_chacha::ChaCha20Rng::from_seed(seed)),
        }
    }
}

impl Rng {
    pub fn random_array<const N: usize>(&self) -> Result<[u8; N], RngError> {
        let mut rng = self.rng.lock().map_err(|_| RngError::LockPoisoned)?;
        let mut out = [0u8; N];
        rng.try_fill_bytes(&mut out)
            .map_err(|_| RngError::NotEnoughRandomness)?;
        Ok(out)
    }

    pub fn random_vec(&self, len: usize) -> Result<Vec<u8>, RngError> {
        let mut rng = self.rng.lock().map_err(|_| RngError::LockPoisoned)?;
        let mut out = vec![0u8; len];
        rng.try_fill_bytes(&mut out)
            .map_err(|_| RngError::NotEnoughRandomness)?;
        Ok(out)
    }
}

#[derive(Debug, Error)]
pub enum RngError {
    #[error("rng lock is poisoned")]
    LockPoisoned,

    #[error("unable to collect enough randomness")]
    NotEnoughRandomness,
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn deterministic_randomness() {
        let sample_1 = {
            let rng = Rng::from_seed([1; 32]);
            rng.random_vec(128).unwrap()
        };

        let sample_2 = {
            let rng = Rng::from_seed([1; 32]);
            rng.random_vec(128).unwrap()
        };

        assert_eq!(sample_1, sample_2);
    }
}
