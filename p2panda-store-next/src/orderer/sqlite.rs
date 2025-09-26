// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;

use sqlx::query;

use crate::orderer::OrdererStore;
use crate::sqlite::{SqliteError, SqlitePool};

impl<'a, T> OrdererStore<T> for SqlitePool<'a> {
    type Error = SqliteError;

    async fn mark_ready(&self, _key: T) -> Result<bool, Self::Error> {
        self.tx(async |tx| {
            // @TODO: Finalize this query.
            query("COUNT()").execute(&mut **tx).await?;
            Ok(true)
        })
        .await
    }

    async fn mark_pending(&self, _key: T, _dependencies: Vec<T>) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn get_next_pending(&self, _key: T) -> Result<Option<HashSet<(T, Vec<T>)>>, Self::Error> {
        todo!()
    }

    async fn take_next_ready(&self) -> Result<Option<T>, Self::Error> {
        todo!()
    }

    async fn remove_pending(&self, _key: T) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn ready(&self, _keys: &[T]) -> Result<bool, Self::Error> {
        todo!()
    }
}
