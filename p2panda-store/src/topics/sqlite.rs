// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{LogId, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as};

use crate::sqlite::{DecodeError, SqliteError, SqliteStore};
use crate::topics::TopicStore;

/// SQLite `TopicStore` implementation that can be used to map a topic to a set of (generic)
/// per-author data identifiers.
impl<T, L> TopicStore<T, VerifyingKey, L> for SqliteStore
where
    T: Serialize + for<'de> Deserialize<'de>,
    L: LogId,
{
    type Error = SqliteError;

    /// Associate a topic with an author + log id pair.
    async fn associate(
        &self,
        topic: &T,
        author: &VerifyingKey,
        data_id: &L,
    ) -> Result<bool, SqliteError> {
        let result = self
            .tx(async |tx| {
                query(
                    "
                    INSERT OR IGNORE
                    INTO
                        topics_v1 (
                            topic,
                            author,
                            data_id
                        )
                    VALUES
                        (?, ?, ?)
                    ",
                )
                .bind(
                    encode_cbor(&topic)
                        .map_err(|err| SqliteError::Encode("topic".to_string(), err))?,
                )
                .bind(author.to_string())
                .bind(
                    encode_cbor(&data_id)
                        .map_err(|err| SqliteError::Encode("data_id".to_string(), err))?,
                )
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Remove an association between a topic and author + log id pair.
    async fn remove(
        &self,
        topic: &T,
        author: &VerifyingKey,
        data_id: &L,
    ) -> Result<bool, SqliteError> {
        let result = self
            .tx(async |tx| {
                query(
                    "
                    DELETE FROM
                        topics_v1
                    WHERE
                        topic = ?
                        AND author = ?
                        AND data_id = ?
                    ",
                )
                .bind(
                    encode_cbor(&topic)
                        .map_err(|err| SqliteError::Encode("topic".to_string(), err))?,
                )
                .bind(author.to_string())
                .bind(
                    encode_cbor(&data_id)
                        .map_err(|err| SqliteError::Encode("data_id".to_string(), err))?,
                )
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Retrieve a list of all logs associated with the provided topic for all known authors.
    async fn resolve(&self, topic: &T) -> Result<BTreeMap<VerifyingKey, Vec<L>>, Self::Error> {
        let data_ids = self
            .execute(async |pool| {
                query_as::<_, (String, Vec<u8>)>(
                    "
                    SELECT
                        author,
                        data_id
                    FROM
                        topics_v1
                    WHERE
                        topic = ?
                    ",
                )
                .bind(
                    encode_cbor(&topic)
                        .map_err(|err| SqliteError::Encode("topic".to_string(), err))?,
                )
                .fetch_all(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        let mut result: BTreeMap<VerifyingKey, Vec<L>> = BTreeMap::new();

        for (author, data_id) in data_ids {
            let author: VerifyingKey = author
                .parse()
                .map_err(|_| SqliteError::Decode("author".into(), DecodeError::FromStr))?;

            let data_id = decode_cbor(&data_id[..])
                .map_err(|err| SqliteError::Decode("data_id".into(), err.into()))?;

            // All items in the returned data set will be unique due to the SQL UNIQUE constraint.
            result.entry(author).or_default().push(data_id);
        }

        Ok(result)
    }
}
