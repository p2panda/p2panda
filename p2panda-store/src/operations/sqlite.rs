// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::hash::{Hash, HashError};
use p2panda_core::{Extensions, LogId, Operation};
use sqlx::{FromRow, query, query_as};

use crate::operations::OperationStore;
use crate::sqlite::{SqliteError, SqliteStore};

const GET_OPERATION: &str = "
    SELECT
        hash,
        header,
        body
    FROM
        operations_v1
    WHERE
        hash = ?
";

const HAS_OPERATION: &str = "
    SELECT
        1
    FROM
        operations_v1
    WHERE
        hash = ?
";

impl<E, L> OperationStore<Operation<E>, Hash, L> for SqliteStore
where
    E: Extensions,
    L: LogId,
{
    type Error = SqliteError;

    async fn insert_operation(
        &self,
        id: &Hash,
        operation: &Operation<E>,
        log_id: &L,
    ) -> Result<bool, Self::Error> {
        let result = self
            .tx(async |tx| {
                query(
                    "
                    INSERT OR IGNORE
                    INTO
                        operations_v1 (
                            hash,
                            log_id,
                            version,
                            public_key,
                            signature,
                            payload_size,
                            payload_hash,
                            timestamp,
                            seq_num,
                            header,
                            header_size,
                            body
                        )
                    VALUES
                        (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ",
                )
                .bind(id.to_hex())
                .bind(
                    encode_cbor(&log_id)
                        .map_err(|err| SqliteError::Encode("log id".to_string(), err))?,
                )
                .bind(operation.header.version.to_string())
                .bind(operation.header.public_key.to_hex())
                .bind(operation.header.signature.map(|sig| sig.to_hex()))
                .bind(operation.header.payload_size.to_string())
                .bind(operation.header.payload_hash.map(|hash| hash.to_hex()))
                .bind(operation.header.timestamp.to_string())
                .bind(operation.header.seq_num.to_string())
                .bind(
                    encode_cbor(&operation.header)
                        .map_err(|err| SqliteError::Encode("header".to_string(), err))?,
                )
                .bind(operation.header.to_bytes().len().to_string())
                .bind(operation.body().map(|body| body.to_bytes()))
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_operation(&self, id: &Hash) -> Result<Option<Operation<E>>, Self::Error> {
        let result = self
            .execute(async |pool| {
                query_as::<_, OperationRow>(GET_OPERATION)
                    .bind(id.to_hex())
                    .fetch_optional(pool)
                    .await
                    .map_err(SqliteError::Sqlite)
            })
            .await?;

        match result {
            Some(row) => Ok(Some(row.try_into()?)),
            None => Ok(None),
        }
    }

    // TODO: In the future we may be able to remove this `_tx` variant of the query by instead
    // requiring that API users exlicitly handle transactions themselves.
    //
    // See: https://github.com/p2panda/p2panda/issues/1065
    async fn get_operation_tx(&self, id: &Hash) -> Result<Option<Operation<E>>, Self::Error> {
        let result = self
            .tx(async |tx| {
                query_as::<_, OperationRow>(GET_OPERATION)
                    .bind(id.to_hex())
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(SqliteError::Sqlite)
            })
            .await?;

        match result {
            Some(row) => Ok(Some(row.try_into()?)),
            None => Ok(None),
        }
    }

    async fn has_operation(&self, id: &Hash) -> Result<bool, Self::Error> {
        let result = self
            .execute(async |pool| {
                query(HAS_OPERATION)
                    .bind(id.to_hex())
                    .fetch_optional(pool)
                    .await
                    .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(result.is_some())
    }

    async fn has_operation_tx(&self, id: &Hash) -> Result<bool, Self::Error> {
        let result = self
            .tx(async |tx| {
                query(HAS_OPERATION)
                    .bind(id.to_hex())
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(result.is_some())
    }

    async fn delete_operation(&self, id: &Hash) -> Result<bool, Self::Error> {
        let result = self
            .tx(async |tx| {
                query(
                    "
                    DELETE FROM
                        operations_v1
                    WHERE
                        hash = ?
                    ",
                )
                .bind(id.to_hex())
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_operation_payload(&self, id: &Hash) -> Result<bool, Self::Error> {
        let result = query(
            "
            UPDATE
                operations_v1
            SET
                body = NULL
            WHERE
                operations_v1.hash = ?
            ",
        )
        .bind(id.to_hex())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

/// Single operation row as it is inserted in the SQLite database.
#[derive(Clone, Debug, FromRow)]
pub(crate) struct OperationRow {
    hash: String,
    pub(crate) header: Vec<u8>,
    body: Option<Vec<u8>>,
}

impl<E> TryFrom<OperationRow> for Operation<E>
where
    E: Extensions,
{
    type Error = SqliteError;

    fn try_from(row: OperationRow) -> Result<Self, Self::Error> {
        Ok(Operation {
            hash: row
                .hash
                .parse()
                .map_err(|err: HashError| SqliteError::Decode("hash".to_string(), err.into()))?,
            header: decode_cbor(&row.header[..])
                .map_err(|err| SqliteError::Decode("header".into(), err.into()))?,
            body: row.body.map(|body| body.into()),
        })
    }
}
