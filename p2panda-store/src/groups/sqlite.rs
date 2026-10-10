// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::group::GroupCrdtState;
use p2panda_auth::traits::{Conditions, Operation as AuthOperation};
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Hash, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Sqlite, query, query_scalar};

use crate::groups::traits::GroupsStore;
use crate::{SqliteError, SqliteStore};

impl<M, C> GroupsStore<M, C> for SqliteStore
where
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
    M: AuthOperation<VerifyingKey, Hash, C> + Serialize + for<'a> Deserialize<'a>,
{
    type Error = SqliteError;

    async fn get_groups_state(
        &self,
        id: Hash,
    ) -> Result<Option<GroupCrdtState<VerifyingKey, Hash, M, C>>, SqliteError> {
        let bytes = self
            .execute(async |pool| get_groups_state(pool, id).await)
            .await?;
        decode_groups_state(bytes)
    }

    async fn get_groups_state_tx(
        &self,
        id: Hash,
    ) -> Result<Option<GroupCrdtState<VerifyingKey, Hash, M, C>>, SqliteError> {
        let bytes = self
            .tx(async |tx| get_groups_state(&mut **tx, id).await)
            .await?;
        decode_groups_state(bytes)
    }

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
}

async fn get_groups_state<'e, E>(executor: E, id: Hash) -> Result<Option<Vec<u8>>, SqliteError>
where
    E: Executor<'e, Database = Sqlite>,
{
    query_scalar(
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
    .fetch_optional(executor)
    .await
    .map_err(SqliteError::Sqlite)
}

fn decode_groups_state<M, C>(
    bytes: Option<Vec<u8>>,
) -> Result<Option<GroupCrdtState<VerifyingKey, Hash, M, C>>, SqliteError>
where
    C: Conditions + for<'a> Deserialize<'a>,
    M: AuthOperation<VerifyingKey, Hash, C> + for<'a> Deserialize<'a>,
{
    let Some(state_bytes) = bytes else {
        return Ok(None);
    };

    let state = decode_cbor(&state_bytes[..])
        .map_err(|err| SqliteError::Decode("state".into(), err.into()))?;

    Ok(Some(state))
}
