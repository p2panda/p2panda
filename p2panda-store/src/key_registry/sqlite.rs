// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_encryption::key_registry::KeyRegistryState;
use p2panda_spaces::ActorId;
use sqlx::{query, query_scalar};

use crate::key_registry::traits::KeyRegistryStore;
use crate::{SqliteError, SqliteStore};

impl KeyRegistryStore for SqliteStore {
    type Error = SqliteError;

    async fn get_key_registry(&self) -> Result<Option<KeyRegistryState<ActorId>>, Self::Error> {
        let state_bytes: Option<Vec<u8>> = self
            .execute(async |pool| {
                query_scalar(
                    "
                SELECT
                    state
                FROM
                    key_registry_v1
                ",
                )
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        if let Some(bytes) = state_bytes {
            let state: KeyRegistryState<ActorId> = decode_cbor(&bytes[..])
                .map_err(|err| SqliteError::Decode("state".into(), err.into()))?;

            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    async fn set_key_registry(&self, state: &KeyRegistryState<ActorId>) -> Result<(), Self::Error> {
        self.tx(async |tx| {
            query(
                "
                INSERT OR REPLACE
                    INTO
                        key_registry_v1(state)
                    VALUES
                        (?)
                ",
            )
            .bind(encode_cbor(&state).map_err(|err| SqliteError::Encode("state".to_string(), err))?)
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }
}
