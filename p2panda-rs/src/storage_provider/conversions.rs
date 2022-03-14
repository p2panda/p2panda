// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Debug;

/// Trait implemented on all types which are to be sent to storage.
pub trait ToStorage<Input>: Sized {
    /// The error type
    type ToMemoryStoreError: Debug;
    /// Returns a data store friendly conversion of this type.
    fn to_store_value(input: Input) -> Result<Self, Self::ToMemoryStoreError>;
}

/// Trait implemented on all implementation specific types which are retrieved
/// from memory store.
pub trait FromStorage<Output> {
    /// The error type
    type FromStorageError: Debug;
    /// Returns a returns the in memory (probably a p2panda_rs type) conversion of this type.
    fn from_store_value(&self) -> Result<Output, Self::FromStorageError>;
}
