// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::marker::PhantomData;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Extensions, Hash, Operation};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as};

use crate::groups::GroupsStore;
use crate::operations::OperationStore;
use crate::spaces::traits::SpacesMessageStore;
use crate::spaces::{SpacesMessage, SpacesStore, SpacesStoreWrite};
use crate::sqlite::TransactionPermit;
use crate::{SqliteError, SqliteStore};

/// Concrete SqliteSpacesStore type which wraps the SqliteStore.
pub struct SqliteSpacesStore<E> {
    store: SqliteStore,
    _phantom: PhantomData<E>,
}

impl<E> SqliteSpacesStore<E> {
    pub fn new(store: SqliteStore) -> Self {
        Self {
            store,
            _phantom: PhantomData,
        }
    }

    pub fn inner(&self) -> SqliteStore {
        self.store.clone()
    }
}

impl<A, E> SpacesMessageStore<Hash, A> for SqliteSpacesStore<E>
where
    A: Clone,
    E: Extensions + Borrow<A>,
{
    type Error = SqliteError;

    async fn get_spaces_message(&self, id: &Hash) -> Result<Option<SpacesMessage<A>>, Self::Error> {
        match <SqliteStore as OperationStore<Operation<E>, Hash>>::get_operation(&self.store, id)
            .await?
        {
            Some(operation) => {
                let args = operation.header().extensions.borrow().clone();
                let message = SpacesMessage {
                    id: operation.hash,
                    author: operation.header().verifying_key,
                    args,
                };
                Ok(Some(message))
            }
            None => Ok(None),
        }
    }
}

impl<ID, S, E> SpacesStore<ID, S> for SqliteSpacesStore<E>
where
    ID: for<'a> Deserialize<'a> + Serialize,
    S: for<'a> Deserialize<'a> + Serialize,
{
    type Error = SqliteError;

    async fn get_space_state_tx(&self, id: &ID) -> Result<Option<S>, Self::Error> {
        let row = self
            .store
            .tx(async |tx| {
                query_as::<_, (Vec<u8>,)>(
                    "
                    SELECT
                        state
                    FROM
                        spaces_v1
                    WHERE
                        id = ?
                    ",
                )
                .bind(encode_cbor(&id).map_err(|err| SqliteError::Encode("id".to_string(), err))?)
                .fetch_optional(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        let Some((state_bytes,)) = row else {
            return Ok(None);
        };

        let state = decode_cbor(&state_bytes[..])
            .map_err(|err| SqliteError::Decode("state".into(), err.into()))?;

        Ok(Some(state))
    }

    async fn has_space(&self, id: &ID) -> Result<bool, Self::Error> {
        let result = self
            .store
            .execute(async |pool| {
                query_as::<_, (Vec<u8>,)>(
                    "
                    SELECT
                        id
                    FROM
                        spaces_v1
                    WHERE
                        id = ?
                    ",
                )
                .bind(encode_cbor(&id).map_err(|err| SqliteError::Encode("id".to_string(), err))?)
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(result.is_some())
    }

    async fn space_ids(&self) -> Result<Vec<ID>, Self::Error> {
        let result = self
            .store
            .execute(async |pool| {
                query_as::<_, (Vec<u8>,)>(
                    "
                    SELECT
                        id
                    FROM
                        spaces_v1
                    ",
                )
                .fetch_all(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        let result: Result<Vec<ID>, _> = result
            .into_iter()
            .map(|(id_bytes,)| decode_cbor::<ID, _>(&id_bytes[..]))
            .collect();

        result.map_err(|err| SqliteError::Decode("state".into(), err.into()))
    }
}

impl<ID, S, E> SpacesStoreWrite<ID, S> for SqliteSpacesStore<E>
where
    ID: for<'a> Deserialize<'a> + Serialize,
    S: for<'a> Deserialize<'a> + Serialize,
{
    type Error = SqliteError;

    async fn set_space_state_tx(&self, id: &ID, state: &S) -> Result<(), Self::Error> {
        self.store
            .tx(async |tx| {
                query(
                    "
                INSERT OR REPLACE
                INTO
                    spaces_v1 (
                        id,
                        state
                    )
                VALUES
                    (?, ?)
                ",
                )
                .bind(encode_cbor(&id).map_err(|err| SqliteError::Encode("id".to_string(), err))?)
                .bind(
                    encode_cbor(&state)
                        .map_err(|err| SqliteError::Encode("state".to_string(), err))?,
                )
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(())
    }
}

impl<ID, S, E> GroupsStore<ID, S> for SqliteSpacesStore<E>
where
    ID: for<'a> Deserialize<'a> + Serialize,
    S: for<'a> Deserialize<'a> + Serialize,
{
    type Error = SqliteError;

    async fn set_groups_state_tx(&self, id: &ID, state: &S) -> Result<(), SqliteError> {
        self.store.set_groups_state_tx(id, state).await
    }

    async fn get_groups_state_tx(&self, id: &ID) -> Result<Option<S>, SqliteError> {
        self.store.get_groups_state_tx(id).await
    }
}

impl<E> crate::traits::Transaction for SqliteSpacesStore<E> {
    type Error = SqliteError;

    type Permit = TransactionPermit;

    async fn begin(&self) -> Result<TransactionPermit, SqliteError> {
        self.store.begin().await
    }

    async fn rollback(&self, permit: TransactionPermit) -> Result<(), SqliteError> {
        self.store.rollback(permit).await
    }

    async fn commit(&self, permit: TransactionPermit) -> Result<(), SqliteError> {
        self.store.commit(permit).await
    }
}
