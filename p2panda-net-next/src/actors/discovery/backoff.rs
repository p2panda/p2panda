// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(test)]
use mock_instant::thread_local::Instant;
use std::time::Duration;
#[cfg(not(test))]
use std::time::Instant;

use rand::Rng;
use rand_chacha::ChaCha20Rng;
use tracing::trace;

/// Simple, incremental backoff logic.
///
/// It starts at an initial value and gets incremented by another, random value, until it hits a
/// ceiling. Another random parameter controls when the backoff gets reset.
#[derive(Debug)]
pub struct Backoff {
    value: Duration,
    last_reset_at: Instant,
    reset_after: Duration,
    config: Config,
    rng: ChaCha20Rng,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Backoff will always begin with this initial value.
    ///
    /// Defaults to 0 / no backoff.
    initial_value: Duration,

    /// Minimum increment value when increasing backoff value.
    min_increment: Duration,

    /// Maximum increment value when increasing backoff value.
    max_increment: Duration,

    /// Maximum reachable backoff value.
    max_value: Duration,

    /// Minimum waiting time until backoff will be reset to initial value.
    min_reset: Duration,

    /// Maximum waiting time until backoff will be reset to initial value.
    max_reset: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            initial_value: Duration::from_secs(0),
            min_increment: Duration::from_millis(50),
            max_increment: Duration::from_millis(250),
            max_value: Duration::from_secs(5),
            min_reset: Duration::from_secs(30),
            max_reset: Duration::from_secs(60),
        }
    }
}

impl Backoff {
    pub fn new(config: Config, rng: ChaCha20Rng) -> Self {
        let mut backoff = Self {
            value: config.initial_value,
            last_reset_at: Instant::now(),
            reset_after: Duration::default(),
            config,
            rng,
        };
        backoff.reset();
        backoff
    }

    pub fn increment(&mut self) {
        // Increment backoff by random value within configured range until it reached maximum.
        if self.value > self.config.max_value {
            self.value = self.config.max_value;
        } else if self.value < self.config.max_value {
            let increment = self.random_increment();
            self.value += increment;
        }

        // Reset backoff after we've waited long enough.
        if self.last_reset_at.elapsed() >= self.reset_after {
            self.reset();
        }
    }

    pub async fn sleep(&self) {
        if self.value.is_zero() {
            return;
        }

        trace!("backoff {} seconds", self.value.as_secs());
        tokio::time::sleep(self.value).await;
    }

    pub fn reset(&mut self) {
        self.value = self.config.initial_value;
        self.last_reset_at = Instant::now();
        self.reset_after = self.random_reset_after();
    }

    fn random_increment(&mut self) -> Duration {
        let range = self.rng.random_range::<u128, _>(
            self.config.min_increment.as_millis()..self.config.max_increment.as_millis(),
        );

        Duration::from_millis(range as u64)
    }

    fn random_reset_after(&mut self) -> Duration {
        let range = self.rng.random_range::<u128, _>(
            self.config.min_reset.as_millis()..self.config.max_reset.as_millis(),
        );

        Duration::from_millis(range as u64)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mock_instant::thread_local::MockClock;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;

    use super::{Backoff, Config};

    #[test]
    fn increment() {
        let config = Config::default();
        let rng = ChaCha20Rng::from_seed([1; 32]);

        let mut backoff = Backoff::new(config.clone(), rng);

        // Backoff should be at initial value in the beginning.
        assert_eq!(backoff.value, config.initial_value);

        let mut last_value = backoff.value.clone();
        let mut last_increment = Duration::default();
        for _ in 0..10 {
            backoff.increment();

            // Increments should gradually increase backoff.
            assert!(last_value < backoff.value);

            // Increments should be within configured range.
            assert!(backoff.value - last_value >= config.min_increment);
            assert!(backoff.value - last_value <= config.max_increment);

            // Increments should be random.
            assert_ne!(backoff.value - last_value, last_increment);

            last_increment = backoff.value - last_value;
            last_value = backoff.value.clone();
        }

        // Force backoff to reach maximum by incrementing it many times.
        for _ in 0..100 {
            backoff.increment();
        }
        assert_eq!(backoff.value, config.max_value);
    }

    #[test]
    fn reset() {
        let config = Config::default();
        let rng = ChaCha20Rng::from_seed([1; 32]);

        let mut backoff = Backoff::new(config.clone(), rng);

        for _ in 0..10 {
            let last_reset_after = backoff.reset_after.clone();
            backoff.reset();

            // Reset should bring up a new, random "reset_after" value.
            assert_ne!(backoff.reset_after, last_reset_after);

            // Reset should bring it back to initial value.
            assert_eq!(backoff.value, config.initial_value);

            // Waiting time until next reset should be within configured range.
            assert!(backoff.reset_after >= config.min_reset);
            assert!(backoff.reset_after <= config.max_reset);
        }

        let last_reset_after = backoff.reset_after.clone();

        // Advance time to a moment right _before_ we want to reset.
        MockClock::advance(config.min_reset - Duration::from_secs(1));

        // Trigger to update its state.
        backoff.increment();

        // We should not reset anything yet.
        assert_eq!(last_reset_after, backoff.reset_after);

        // Advance time to a moment right _after_ we want to reset.
        MockClock::advance(config.max_reset + Duration::from_secs(1));

        // Trigger to update its state.
        backoff.increment();

        // Backoff was reset as it reached the max. waiting time.
        assert_ne!(last_reset_after, backoff.reset_after);
    }
}
