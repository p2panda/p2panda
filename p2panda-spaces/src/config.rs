// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::time::Duration;

use p2panda_encryption::key_bundle::Lifetime;
use serde::{Deserialize, Serialize};

use crate::Credentials;

/// Configuration for a spaces instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Credentials (private key & identity secret) for local peer.
    pub(crate) credentials: Credentials,

    /// When a key bundle should be considered expired and thus invalid.
    pub(crate) pre_key_lifetime: Duration,

    /// Rotate our own pre keys after this duration, to allow some time between peers receiving
    /// our new one and the old one expiring.
    pub(crate) pre_key_rotate_after: Duration,
}

impl Config {
    pub fn new(credentials: &Credentials) -> Self {
        Self {
            credentials: credentials.to_owned(),
            pre_key_lifetime: Duration::from_secs(60 * 60 * 24 * 90), // 90 days
            pre_key_rotate_after: Duration::from_secs(60 * 60 * 24 * 60), // 60 days
        }
    }

    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    pub(crate) fn lifetime(&self) -> Lifetime {
        Lifetime::new(self.pre_key_lifetime.as_secs())
    }
}
