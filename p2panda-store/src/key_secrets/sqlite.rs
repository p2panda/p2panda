// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_scalar};

use crate::key_secrets::traits::KeySecretsStore;
use crate::{SqliteError, SqliteStore};

// Constant identifier used to provide a primary key for the database table.
// This makes it possible to use INSERT OR REPLACE to update the prekey secrets state.
const DEFAULT: &str = "prekey_secrets_state_id";

impl<S> KeySecretsStore<S> for SqliteStore
where
    S: for<'a> Deserialize<'a> + Serialize,
{
    type Error = SqliteError;

    async fn get_prekey_secrets(&self) -> Result<Option<S>, SqliteError> {
        let state_bytes: Option<Vec<u8>> = self
            .execute(async |pool| {
                query_scalar(
                    "
                    SELECT
                        state
                    FROM
                        key_secrets_v1
                    WHERE
                        id = ?
                    ",
                )
                .bind(DEFAULT)
                .fetch_optional(pool)
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

    async fn set_prekey_secrets(&self, state: &S) -> Result<(), SqliteError> {
        self.tx(async |tx| {
            query(
                "
                INSERT OR REPLACE
                    INTO
                        key_secrets_v1(id, state)
                    VALUES
                        (?, ?)
                ",
            )
            .bind(DEFAULT)
            .bind(encode_cbor(&state).map_err(|err| SqliteError::Encode("state".to_string(), err))?)
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }
}
