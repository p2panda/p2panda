// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::marker::PhantomData;
use std::str::FromStr;

use p2panda_auth::group::GroupCrdtState;
use p2panda_auth::traits::{Conditions, Operation as AuthOperation};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Extensions, Hash, Operation, VerifyingKey};
use p2panda_encryption::key_manager::PreKeyBundlesState;
use p2panda_encryption::key_registry::KeyRegistryState;
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as};

use crate::groups::GroupsStore;
use crate::key_registry::KeyRegistryStore;
use crate::key_secrets::KeySecretsStore;
use crate::operations::OperationStore;
use crate::spaces::traits::SpacesMessageStore;
use crate::spaces::{SpacesMessage, SpacesStore};
use crate::sqlite::TransactionPermit;
use crate::{SqliteError, SqliteStore};

#[derive(Clone)]
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
}

impl<ARG, E> SpacesMessageStore<ARG> for SqliteSpacesStore<E>
where
    ARG: Clone,
    E: Extensions + Borrow<ARG>,
{
    type Error = SqliteError;

    async fn get_spaces_message(
        &self,
        id: &Hash,
    ) -> Result<Option<SpacesMessage<ARG>>, Self::Error> {
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

impl<S, E> SpacesStore<S> for SqliteSpacesStore<E>
where
    S: for<'a> Deserialize<'a> + Serialize,
{
    type Error = SqliteError;

    async fn get_space_state_tx(&self, id: &Hash) -> Result<Option<S>, Self::Error> {
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
                .bind(id.to_hex())
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

    async fn set_space_state_tx(&self, id: &Hash, state: &S) -> Result<(), Self::Error> {
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
                .bind(id.to_hex())
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

    async fn has_space(&self, id: &Hash) -> Result<bool, Self::Error> {
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
                .bind(id.to_hex())
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(result.is_some())
    }

    async fn space_ids(&self) -> Result<Vec<Hash>, Self::Error> {
        let result = self
            .store
            .execute(async |pool| {
                query_as::<_, (String,)>(
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

        let result: Result<Vec<Hash>, _> = result
            .iter()
            .map(|(id_str,)| Hash::from_str(id_str))
            .collect();

        result.map_err(|err| SqliteError::Decode("state".into(), err.into()))
    }
}

impl<E> KeyRegistryStore for SqliteSpacesStore<E> {
    type Error = SqliteError;

    async fn get_key_registry(
        &self,
    ) -> Result<Option<KeyRegistryState<VerifyingKey>>, Self::Error> {
        self.store.get_key_registry().await
    }

    async fn set_key_registry(
        &self,
        state: &KeyRegistryState<VerifyingKey>,
    ) -> Result<(), Self::Error> {
        self.store.set_key_registry(state).await
    }
}

impl<E> KeySecretsStore for SqliteSpacesStore<E> {
    type Error = SqliteError;

    async fn get_prekey_secrets(&self) -> Result<Option<PreKeyBundlesState>, Self::Error> {
        self.store.get_prekey_secrets().await
    }

    async fn set_prekey_secrets(&self, state: &PreKeyBundlesState) -> Result<(), Self::Error> {
        self.store.set_prekey_secrets(state).await
    }
}

impl<E, M, C> GroupsStore<M, C> for SqliteSpacesStore<E>
where
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
    M: AuthOperation<VerifyingKey, Hash, C> + Serialize + for<'a> Deserialize<'a>,
{
    type Error = SqliteError;

    async fn set_groups_state_tx(
        &self,
        id: Hash,
        state: &GroupCrdtState<VerifyingKey, Hash, M, C>,
    ) -> Result<(), SqliteError> {
        self.store.set_groups_state_tx(id, state).await
    }

    async fn get_groups_state_tx(
        &self,
        id: Hash,
    ) -> Result<Option<GroupCrdtState<VerifyingKey, Hash, M, C>>, SqliteError> {
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
