// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::VerifyingKey;
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_encryption::key_registry::KeyRegistryState;
use sqlx::{query, query_scalar};

use crate::key_registry::traits::KeyRegistryStore;
use crate::{SqliteError, SqliteStore};

// Constant identifier used to provide a primary key for the database table. This makes it possible
// to only ever store one row into the table and use INSERT OR REPLACE to update the state.
const DEFAULT_KEY_REGISTRY: &str = "default";

impl KeyRegistryStore for SqliteStore {
    type Error = SqliteError;

    async fn get_key_registry_tx(
        &self,
    ) -> Result<Option<KeyRegistryState<VerifyingKey>>, Self::Error> {
        let state_bytes: Option<Vec<u8>> = self
            .tx(async |tx| {
                query_scalar(
                    "
                    SELECT
                        state
                    FROM
                        key_registry_v1
                    WHERE
                        id = ?
                    ",
                )
                .bind(DEFAULT_KEY_REGISTRY)
                .fetch_optional(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        if let Some(bytes) = state_bytes {
            let state = decode_cbor(&bytes[..])
                .map_err(|err| SqliteError::Decode("state".into(), err.into()))?;
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    async fn set_key_registry_tx(
        &self,
        state: &KeyRegistryState<VerifyingKey>,
    ) -> Result<(), Self::Error> {
        let state_bytes =
            encode_cbor(&state).map_err(|err| SqliteError::Encode("state".to_string(), err))?;

        self.tx(async |tx| {
            query(
                "
                INSERT OR REPLACE
                    INTO
                        key_registry_v1(id, state)
                    VALUES
                        (?, ?)
                ",
            )
            .bind(DEFAULT_KEY_REGISTRY)
            .bind(state_bytes)
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }
}
