// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt;

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use zeroize::ZeroizeOnDrop;

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
        bool::from(self.0.ct_eq(&other.0))
    }
}

#[cfg(not(test))]
impl<const N: usize> fmt::Debug for Secret<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SeretKey").field(&"***").finish()
    }
}
