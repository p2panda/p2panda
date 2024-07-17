// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_core::{Extension, Hash, Operation, PublicKey};
use thiserror::Error;

use crate::LogId;

pub trait OperationStore<E>
where
    E: Extension<LogId>,
{
    type LogId;

    /// Insert an operation.
    ///
    /// Returns `true` when the insert occurred, or `false` when the operation
    /// already existed and no insertion occurred.
    fn insert_operation(&mut self, operation: Operation<E>) -> Result<bool, StoreError>;

    /// Get an operation.
    fn get_operation(&self, hash: Hash) -> Result<Option<Operation<E>>, StoreError>;

    /// Delete an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation
    /// was not found in the store.
    fn delete_operation(&mut self, hash: Hash) -> Result<bool, StoreError>;

    /// Delete the payload of an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation
    /// was not found in the store or the payload was already deleted.
    fn delete_payload(&mut self, hash: Hash) -> Result<bool, StoreError>;
}

pub trait LogStore<E>
where
    E: Extension<LogId>,
{
    type LogId;

    fn get_log(
        &self,
        public_key: PublicKey,
        log_id: LogId,
    ) -> Result<Option<Vec<Operation<E>>>, StoreError>;

    fn latest_operation(
        &self,
        public_key: PublicKey,
        log_id: LogId,
    ) -> Result<Option<Operation<E>>, StoreError>;

    fn delete_operations(
        &mut self,
        public_key: PublicKey,
        log_id: LogId,
        from: u64,
        to: Option<u64>,
    ) -> Result<(), StoreError>;

    fn delete_payloads(
        &mut self,
        public_key: PublicKey,
        log_id: LogId,
        from: u64,
        to: Option<u64>,
    ) -> Result<(), StoreError>;
}

pub trait StreamStore<E>
where
    E: Extension<Self::StreamId>,
{
    type StreamId;

    fn get_stream(stream_name: Self::StreamId) -> Result<Option<Vec<Operation<E>>>, StoreError>;
}

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Error occurred in OperationStore: {0}")]
    OperationStoreError(String),
}
