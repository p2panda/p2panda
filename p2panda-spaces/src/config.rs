// SPDX-License-Identifier: MIT OR Apache-2.0

//! Serializable configuration object.
use std::fmt::Debug;
use std::time::Duration;

#[cfg(any(test, feature = "test_utils"))]
use p2panda_encryption::key_bundle::Lifetime;
use serde::{Deserialize, Serialize};

/// Configuration for a spaces instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// When a key bundle should be considered expired and thus invalid.
    pub pre_key_lifetime: Duration,

    /// Rotate our own pre keys after this duration, to allow some time between peers receiving
    /// our new one and the old one expiring.
    pub pre_key_rotate_after: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            pre_key_lifetime: Duration::from_secs(60 * 60 * 24 * 90), // 90 days
            pre_key_rotate_after: Duration::from_secs(60 * 60 * 24 * 60), // 60 days
        }
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl Config {
    pub(crate) fn lifetime(&self) -> Lifetime {
        Lifetime::new(self.pre_key_lifetime.as_secs())
    }
}
