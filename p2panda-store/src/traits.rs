// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_core::{Body, Hash, Header, Operation, PublicKey};
use serde::de::DeserializeOwned;
use serde::Serialize;
use thiserror::Error;

pub trait OperationStore<E>
where
    E: Clone + Serialize + DeserializeOwned,
{
    /// Insert an operation.
    ///
    /// Returns `true` when the insert occurred, or `false` when the operation
    /// already existed and no insertion occurred.
    fn insert(header: Header<E>, body: Body) -> Result<bool, StoreError>;

    /// Get a single operation.
    fn get(hash: &Hash) -> Result<Option<Operation<E>>, StoreError>;

    /// Remove a single operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation
    /// was not found in the store.
    fn remove(hash: &Hash) -> Result<bool, StoreError>;

    /// Get all operations from a single authors log.
    ///
    /// Returns `None` if the requested log or author was not found in the store.
    fn all(public_key: &PublicKey, log_id: &str) -> Result<Option<Vec<Operation<E>>>, StoreError>;
}

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Error occurred in OperationStore: {0}")]
    OperationStoreError(String),
}
