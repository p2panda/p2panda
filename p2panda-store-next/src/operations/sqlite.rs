// SPDX-License-Identifier: MIT OR Apache-2.0

use sqlx::query;

use crate::operations::OperationStore;
use crate::sqlite::{SqliteError, SqliteStore};

impl<'a, T, ID> OperationStore<T, ID> for SqliteStore<'a> {
    type Error = SqliteError;

    async fn insert_operation(&self, id: &ID, operation: T) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn get_operation(&self, id: &ID) -> Result<Option<T>, Self::Error> {
        todo!()
    }

    async fn has_operation(&self, id: &ID) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn delete_operation(&self, id: &ID) -> Result<bool, Self::Error> {
        todo!()
    }
}
