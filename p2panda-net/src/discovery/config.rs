// SPDX-License-Identifier: MIT OR Apache-2.0

#[derive(Clone, Debug)]
pub struct DiscoveryConfig {
    /// Number of random walkers which "explore" the network at the same time.
    pub random_walkers_count: usize,

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

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            random_walkers_count: 2,
            reset_walk_probability: 0.02, // 2% chance
        }
    }
}
