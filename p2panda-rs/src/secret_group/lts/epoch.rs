// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use tls_codec::{Size, TlsDeserialize, TlsSerialize, TlsSize};

/// Holds the value of a Long Term Secret epoch starting with zero.
#[derive(
    Debug,
    Clone,
    Copy,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    TlsDeserialize,
    TlsSerialize,
    TlsSize,
)]
pub struct LongTermSecretEpoch(pub u64);

impl Default for LongTermSecretEpoch {
    fn default() -> Self {
        Self(0)
    }
}

impl LongTermSecretEpoch {
    /// Increments the epoch by one.
    pub fn increment(&mut self) {
        self.0 += 1;
    }
}
