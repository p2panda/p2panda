// SPDX-License-Identifier: MIT OR Apache-2.0

#[cfg(not(test))]
use std::fmt;

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use zeroize::ZeroizeOnDrop;

/// Generic container for sensitive bytes with best-effort security measures.
///
/// In particular this implementation provides:
/// 1. Zeroise memory on drop.
/// 2. Private API methods to retrieve bytes, preventing misuse.
/// 3. Hide bytes value when printing debug info.
/// 4. Constant-time comparison implementation to prevent timing attacks.
///
/// This represents a "best-effort" attempt, since side-channels are ultimately a property of a
/// deployed cryptographic system including the hardware it runs on, not just of software.
#[derive(Clone, Eq, Serialize, Deserialize, ZeroizeOnDrop)]
#[cfg_attr(test, derive(Debug))]
pub struct Secret<const N: usize>(#[serde(with = "serde_bytes")] [u8; N]);

impl<const N: usize> Secret<N> {
    pub(crate) fn from_bytes(bytes: [u8; N]) -> Self {
        Self(bytes)
    }

    pub(crate) fn as_bytes(&self) -> &[u8; N] {
        &self.0
    }
}

impl<const N: usize> PartialEq for Secret<N> {
    fn eq(&self, other: &Self) -> bool {
        // Constant-time comparison.
        bool::from(self.0.ct_eq(&other.0))
    }
}

#[cfg(not(test))]
impl<const N: usize> fmt::Debug for Secret<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Do not reveal secret values when printing debug info.
        f.debug_struct("Secret").field("value", &"***").finish()
    }
}
