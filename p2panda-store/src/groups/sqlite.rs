// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::group::GroupCrdtState;
use p2panda_auth::traits::{Conditions, Operation as AuthOperation};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Hash, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::query;
use sqlx::query_as;

use crate::groups::traits::GroupsStore;
use crate::{SqliteError, SqliteStore};

impl<M, C> GroupsStore<M, C> for SqliteStore
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
            .bind(id.to_hex())
            .bind(encode_cbor(&state).map_err(|err| SqliteError::Encode("state".to_string(), err))?)
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }

    async fn get_groups_state_tx(
        &self,
        id: Hash,
    ) -> Result<Option<GroupCrdtState<VerifyingKey, Hash, M, C>>, SqliteError> {
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
}
