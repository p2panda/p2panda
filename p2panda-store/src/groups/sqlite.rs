// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use serde::{Deserialize, Serialize};
use sqlx::query;
use sqlx::query_as;

use crate::groups::traits::GroupsStore;
use crate::{SqliteError, SqliteStore};

impl<ID, S> GroupsStore<ID, S> for SqliteStore
where
    ID: for<'a> Deserialize<'a> + Serialize,
    S: for<'a> Deserialize<'a> + Serialize,
{
    type Error = SqliteError;

    async fn set_state(&self, id: &ID, state: &S) -> Result<(), SqliteError> {
        self.tx(async |tx| {
            query(
                "
                INSERT OR REPLACE
                INTO
                    groups_v1 (
                        id,
                        state
                    )
                VALUES
                    (?, ?)
                ",
            )
            .bind(encode_cbor(&id).map_err(|err| SqliteError::Encode("id".to_string(), err))?)
            .bind(encode_cbor(&state).map_err(|err| SqliteError::Encode("state".to_string(), err))?)
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }

    async fn get_state(&self, id: &ID) -> Result<Option<S>, SqliteError> {
        let row = self
            .tx(async |tx| {
                query_as::<_, (Vec<u8>,)>(
                    "
                    SELECT
                        state
                    FROM
                        groups_v1
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
}
