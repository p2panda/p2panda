// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::hash::{Hash, HashError};
use p2panda_core::{Extensions, Operation};
use sqlx::{FromRow, query, query_as};

use crate::operations::OperationStore;
use crate::sqlite::{SqliteError, SqliteStore};

impl<'a, E> OperationStore<Operation<E>, Hash> for SqliteStore<'a>
where
    E: Extensions,
{
    type Error = SqliteError;

    async fn insert_operation(
        &self,
        id: &Hash,
        operation: Operation<E>,
    ) -> Result<bool, Self::Error> {
        let result = self
            .tx(async |tx| {
                query(
                    "
                    INSERT OR IGNORE
                    INTO
                        operations_v1 (
                            hash,
                            version,
                            public_key,
                            signature,
                            payload_size,
                            payload_hash,
                            timestamp,
                            header,
                            body,
                            extensions
                        )
                    VALUES
                        (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ",
                )
                .bind(id.to_hex())
                .bind(operation.header.version.to_string())
                .bind(operation.header.public_key.to_hex())
                .bind(operation.header.signature.map(|sig| sig.to_hex()))
                .bind(operation.header.payload_size.to_string())
                .bind(operation.header.payload_hash.map(|hash| hash.to_hex()))
                .bind(operation.header.timestamp.to_string())
                .bind(
                    encode_cbor(&operation.header)
                        .map_err(|err| SqliteError::Encode("header".to_string(), err))?,
                )
                .bind(operation.body.map(|body| body.to_bytes()))
                .bind(match operation.header.extensions {
                    Some(ref extensions) => Some(
                        encode_cbor(extensions)
                            .map_err(|err| SqliteError::Encode("extensions".to_string(), err))?,
                    ),
                    None => None,
                })
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
                query_as::<_, OperationRow>(
                    "
                    SELECT
                        hash,
                        header,
                        body
                    FROM
                        operations_v1
                    WHERE
                        hash = ?
                    ",
                )
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

    async fn has_operation(&self, id: &Hash) -> Result<bool, Self::Error> {
        let result = self
            .execute(async |pool| {
                query(
                    "
                    SELECT
                        1
                    FROM
                        operations_v1
                    WHERE
                        hash = ?
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
}

/// Single operation row as it is inserted in the SQLite database.
#[derive(Debug, FromRow)]
struct OperationRow {
    hash: String,
    header: Vec<u8>,
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
