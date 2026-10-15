// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_encryption::key_manager::PreKeyBundlesState;
use sqlx::{Executor, Sqlite, query, query_scalar};

use crate::key_secrets::traits::PreKeyBundlesStore;
use crate::{SqliteError, SqliteStore};

// Constant identifier used to provide a primary key for the database table. This makes it possible
// to only ever store one row into the table and use INSERT OR REPLACE to update the state.
const DEFAULT_PREKEY_BUNDLES_STATE: &str = "default";

impl PreKeyBundlesStore for SqliteStore {
    type Error = SqliteError;

    async fn get_prekey_secrets(&self) -> Result<Option<PreKeyBundlesState>, SqliteError> {
        let bytes = self
            .execute(async |pool| get_prekey_secrets(pool).await)
            .await?;

        let Some(bytes) = bytes else {
            return Ok(None);
        };

        Ok(Some(decode_state(bytes)?))
    }

    async fn get_prekey_secrets_tx(&self) -> Result<Option<PreKeyBundlesState>, SqliteError> {
        let bytes = self
            .tx(async |tx| get_prekey_secrets(&mut **tx).await)
            .await?;

        let Some(bytes) = bytes else {
            return Ok(None);
        };

        Ok(Some(decode_state(bytes)?))
    }

    async fn set_prekey_secrets_tx(&self, state: &PreKeyBundlesState) -> Result<(), SqliteError> {
        let state_bytes =
            encode_cbor(&state).map_err(|err| SqliteError::Encode("state".to_string(), err))?;

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
            .bind(DEFAULT_PREKEY_BUNDLES_STATE)
            .bind(state_bytes)
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }
}

async fn get_prekey_secrets<'e, E>(executor: E) -> Result<Option<Vec<u8>>, SqliteError>
where
    E: Executor<'e, Database = Sqlite>,
{
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
    .bind(DEFAULT_PREKEY_BUNDLES_STATE)
    .fetch_optional(executor)
    .await
    .map_err(SqliteError::Sqlite)
}

fn decode_state(bytes: Vec<u8>) -> Result<PreKeyBundlesState, SqliteError> {
    let state =
        decode_cbor(&bytes[..]).map_err(|err| SqliteError::Decode("state".into(), err.into()))?;
    Ok(state)
}
