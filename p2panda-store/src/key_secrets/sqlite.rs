// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_encryption::key_manager::PreKeyBundlesState;
use sqlx::{query, query_scalar};

use crate::key_secrets::traits::KeySecretsStore;
use crate::{SqliteError, SqliteStore};

impl KeySecretsStore for SqliteStore {
    type Error = SqliteError;

    async fn get_prekey_secrets(&self) -> Result<Option<PreKeyBundlesState>, SqliteError> {
        let state_bytes: Option<Vec<u8>> = self
            .execute(async |pool| {
                query_scalar(
                    "
                SELECT
                    state
                FROM
                    key_secrets_v1
                ",
                )
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        if let Some(bytes) = state_bytes {
            let state: PreKeyBundlesState = decode_cbor(&bytes[..])
                .map_err(|err| SqliteError::Decode("state".into(), err.into()))?;

            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    async fn set_prekey_secrets(&self, state: &PreKeyBundlesState) -> Result<(), SqliteError> {
        self.tx(async |tx| {
            query(
                "
                INSERT OR REPLACE
                    INTO
                        key_secrets_v1(state)
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
