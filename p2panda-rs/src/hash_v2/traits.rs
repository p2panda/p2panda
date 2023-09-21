// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for hash-like structs.
use crate::hash_v2::Hash;

/// Trait implemented on types which are derived from a hash.
pub trait HashId {
    /// Return the hash this id is derived from.
    fn as_hash(&self) -> &Hash;

    /// Returns hash as bytes.
    fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already validated the hash
        hex::decode(self.as_hash().as_str()).unwrap()
    }
}
