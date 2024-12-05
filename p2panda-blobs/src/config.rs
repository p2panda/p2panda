// SPDX-License-Identifier: MIT OR Apache-2.0

//! Alternative configuration API which can be passed into `Blobs::from_builder_with_config` constructor.
use std::time::Duration;

use iroh_blobs::downloader::{ConcurrencyLimits, RetryConfig};

/// Configuration parameters for the blobs service.
#[derive(Clone, Debug)]
pub struct Config {
    /// Maximum number of requests the service performs concurrently.
    pub max_concurrent_requests: usize,
    /// Maximum number of requests performed by a single node concurrently.
    pub max_concurrent_requests_per_node: usize,
    /// Maximum number of open connections the service maintains.
    pub max_open_connections: usize,
    /// Maximum number of nodes to dial concurrently for a single request.
    pub max_concurrent_dials_per_hash: usize,
    /// Maximum number of retry attempts for a node that failed to dial or failed with IO errors.
    pub max_retries_per_node: u32,
    /// The initial delay to wait before retrying a node. On subsequent failures, the retry delay
    /// will be multiplied with the number of failed retries.
    pub initial_retry_delay: Duration,
}

impl Default for Config {
    fn default() -> Self {
        let concurrency_limits = ConcurrencyLimits::default();
        let retry_config = RetryConfig::default();

        Self {
            max_concurrent_requests: concurrency_limits.max_concurrent_requests,
            max_concurrent_requests_per_node: concurrency_limits.max_concurrent_requests_per_node,
            max_open_connections: concurrency_limits.max_open_connections,
            max_concurrent_dials_per_hash: concurrency_limits.max_concurrent_dials_per_hash,
            max_retries_per_node: retry_config.max_retries_per_node,
            initial_retry_delay: retry_config.initial_retry_delay,
        }
    }
}

impl From<Config> for ConcurrencyLimits {
    fn from(val: Config) -> Self {
        ConcurrencyLimits {
            max_concurrent_requests: val.max_concurrent_requests,
            max_concurrent_requests_per_node: val.max_concurrent_requests_per_node,
            max_open_connections: val.max_open_connections,
            max_concurrent_dials_per_hash: val.max_concurrent_dials_per_hash,
        }
    }
}

impl From<Config> for RetryConfig {
    fn from(val: Config) -> Self {
        RetryConfig {
            max_retries_per_node: val.max_retries_per_node,
            initial_retry_delay: val.initial_retry_delay,
        }
    }
}
