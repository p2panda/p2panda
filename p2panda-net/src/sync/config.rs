// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use tokio::time::Duration;

use p2panda_sync::{SyncProtocol, TopicQuery};

const MAX_CONCURRENT_SYNC_SESSIONS: usize = 128;
const MAX_RETRY_ATTEMPTS: u8 = 5;
const RESYNC_INTERVAL: Duration = Duration::from_secs(60);
const RESYNC_POLL_INTERVAL: Duration = Duration::from_secs(3);
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
const RETRY_POLL_INTERVAL: Duration = Duration::from_secs(3);
const SYNC_QUEUE_SEND_TIMEOUT: Duration = Duration::from_millis(100);
pub(crate) const FALLBACK_RESYNC_INTERVAL_SEC: u64 = 3600;

/// Configuration parameters for resync behaviour.
#[derive(Clone, Debug)]
pub struct ResyncConfiguration {
    /// Minimum interval between resync attempts for a single peer-topic combination.
    ///
    /// Default: 60 seconds.
    pub(crate) interval: Duration,

    /// Minimum interval between each poll of the resync queue.
    ///
    /// Default: 3 second.
    pub(crate) poll_interval: Duration,
}

impl ResyncConfiguration {
    /// Return a default instance of `ResyncConfiguration`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Define the minimum number of seconds between resync attempts for a single peer-topic
    /// combination.
    pub fn interval(mut self, seconds: u64) -> Self {
        self.interval = Duration::from_secs(seconds);
        self
    }

    /// Define the minimum number of seconds between poll of the resync queue.
    pub fn poll_interval(mut self, seconds: u64) -> Self {
        self.poll_interval = Duration::from_secs(seconds);
        self
    }
}

impl Default for ResyncConfiguration {
    fn default() -> Self {
        ResyncConfiguration {
            interval: RESYNC_INTERVAL,
            poll_interval: RESYNC_POLL_INTERVAL,
        }
    }
}

/// Configuration parameters for data synchronisation between peers.
#[derive(Clone, Debug)]
pub struct SyncConfiguration<T> {
    protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,

    /// Resync configuration (`None` represents no resync).
    pub(crate) resync: Option<ResyncConfiguration>,

    /// Maximum number of concurrent sync sessions.
    ///
    /// Default: 128.
    pub(crate) max_concurrent_sync_sessions: usize,

    /// Maximum number of attempts at successfully completing a sync session with a specific peer.
    ///
    /// Default: 5.
    pub(crate) max_retry_attempts: u8,

    /// Minimum interval between sync retry attempts (following a failed attempt).
    ///
    /// Default: 5 seconds.
    pub(crate) retry_interval: Duration,

    /// Minimum interval between each poll of the resync queue.
    ///
    /// Default: 3 seconds.
    pub(crate) retry_poll_interval: Duration,

    /// Maximum time to wait for sync attempt queue to have an open slot before failing.
    ///
    /// Default: 100 milliseconds.
    pub(crate) sync_queue_send_timeout: Duration,
}

impl<T> SyncConfiguration<T>
where
    T: TopicQuery,
{
    /// Return a default instance of `SyncConfiguration`.
    pub fn new(protocol: impl for<'a> SyncProtocol<'a, T> + 'static) -> Self {
        Self {
            protocol: Arc::new(protocol),
            max_concurrent_sync_sessions: MAX_CONCURRENT_SYNC_SESSIONS,
            max_retry_attempts: MAX_RETRY_ATTEMPTS,
            resync: None,
            retry_interval: RETRY_INTERVAL,
            retry_poll_interval: RETRY_POLL_INTERVAL,
            sync_queue_send_timeout: SYNC_QUEUE_SEND_TIMEOUT,
        }
    }

    /// Define the maximum number of concurrent sync sessions.
    pub fn max_concurrent_sync_sessions(mut self, sessions: usize) -> Self {
        self.max_concurrent_sync_sessions = sessions;
        self
    }

    /// Define the maximum number of attempts at successfully completing a sync session with a
    /// specific peer.
    pub fn max_retry_attempts(mut self, attempts: u8) -> Self {
        self.max_retry_attempts = attempts;
        self
    }

    /// Return the sync protocol from the given configuration.
    pub fn protocol(&self) -> Arc<dyn for<'a> SyncProtocol<'a, T>> {
        self.protocol.clone()
    }

    /// Provide the resync configuration for the sync scheduler.
    pub fn resync(mut self, config: ResyncConfiguration) -> Self {
        self.resync = Some(config);
        self
    }

    /// Is resync enabled?
    pub fn is_resync(&mut self) -> bool {
        self.resync.is_some()
    }

    /// Define the minimum number of seconds between retry attempts for a single peer-topic
    /// combination.
    pub fn retry_interval(mut self, seconds: u64) -> Self {
        self.retry_interval = Duration::from_secs(seconds);
        self
    }

    /// Define the minimum number of seconds between poll of the retry queue.
    pub fn retry_poll_interval(mut self, seconds: u64) -> Self {
        self.retry_poll_interval = Duration::from_secs(seconds);
        self
    }

    /// Define the maximum number of seconds to wait for sync attempt queue to have an open slot
    /// before failing.
    pub fn sync_queue_send_timeout(mut self, seconds: u64) -> Self {
        self.sync_queue_send_timeout = Duration::from_secs(seconds);
        self
    }
}
