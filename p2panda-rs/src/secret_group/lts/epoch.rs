// SPDX-License-Identifier: AGPL-3.0-or-later

use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

/// Holds the value of a Long Term Secret epoch starting with zero.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    Ord,
    PartialEq,
    PartialOrd,
    TlsDeserialize,
    TlsSerialize,
    TlsSize,
)]
pub struct LongTermSecretEpoch(pub u64);

impl LongTermSecretEpoch {
    /// Increments the epoch by one.
    pub fn increment(&mut self) {
        self.0 += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::LongTermSecretEpoch;

    #[test]
    fn increment() {
        let mut epoch = LongTermSecretEpoch::default();
        assert_eq!(epoch.0, 0);
        epoch.increment();
        assert_eq!(epoch.0, 1);
    }
}
