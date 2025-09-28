// SPDX-License-Identifier: MIT OR Apache-2.0

use std::str::FromStr;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::hash::{Hash, HashError};
use p2panda_core::{Extensions, Header, Operation, PublicKey, Signature};
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
                    INSERT INTO
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
                            extensions,
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
                        .map_err(|err| SqliteError::Encode("header".to_string(), err.into()))?,
                )
                .bind(operation.body.map(|body| body.to_bytes()))
                .bind(match operation.header.extensions {
                    Some(ref extensions) => Some(encode_cbor(extensions).map_err(|err| {
                        SqliteError::Encode("extensions".to_string(), err.into())
                    })?),
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
                        version,
                        public_key,
                        signature,
                        payload_size,
                        payload_hash,
                        timestamp,
                        header,
                        body,
                        extensions,
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
                    DELETE
                    FROM
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
#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct OperationRow {
    hash: String,
    version: String,
    public_key: String,
    signature: String,
    payload_size: String,
    payload_hash: Option<String>,
    timestamp: String,
    header: Vec<u8>,
    body: Option<Vec<u8>>,
    extensions: Option<Vec<u8>>,
}

impl<E> TryFrom<OperationRow> for Operation<E>
where
    E: Extensions,
{
    type Error = SqliteError;

    fn try_from(row: OperationRow) -> Result<Self, Self::Error> {
        let header = Header {
            version: row
                .version
                .parse::<u64>()
                .map_err(|err| SqliteError::Decode("version".to_string(), err.into()))?,
            public_key: PublicKey::from_str(&row.public_key)
                .map_err(|err| SqliteError::Decode("public_key".to_string(), err.into()))?,
            signature: Some(
                Signature::from_str(&row.signature)
                    .map_err(|err| SqliteError::Decode("signature".to_string(), err.into()))?,
            ),
            payload_size: row
                .payload_size
                .parse::<u64>()
                .map_err(|err| SqliteError::Decode("payload_size".to_string(), err.into()))?,
            payload_hash: {
                match row.payload_hash {
                    Some(hash_str) => Some(Hash::from_str(&hash_str).map_err(|err| {
                        SqliteError::Decode("payload_hash".to_string(), err.into())
                    })?),
                    None => None,
                }
            },
            timestamp: row
                .timestamp
                .parse::<u64>()
                .map_err(|err| SqliteError::Decode("timestamp".to_string(), err.into()))?,
            extensions: {
                match row.extensions {
                    Some(bytes) => Some(decode_cbor(&bytes[..]).map_err(|err| {
                        SqliteError::Decode("extensions".to_string(), err.into())
                    })?),
                    None => None,
                }
            },
            // @TODO: These fields will be moved from "header" into "extensions" soon:
            seq_num: 0,
            backlink: None,
            previous: Vec::new(),
        };

        Ok(Operation {
            hash: row
                .hash
                .parse()
                .map_err(|err: HashError| SqliteError::Decode("hash".to_string(), err.into()))?,
            header,
            body: row.body.map(|body| body.into()),
        })
    }
}
