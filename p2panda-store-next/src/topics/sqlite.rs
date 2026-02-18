// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{LogId, PublicKey, Topic};
use sqlx::{query, query_as};

use crate::sqlite::{SqliteError, SqliteStore};
use crate::topics::TopicStore;

/// SQLite `TopicStore` implementation that can be used to map a topic to a set of (generic)
/// per-author data identifiers.
impl<'a, S> TopicStore<Topic, PublicKey, S> for SqliteStore<'a>
where
    S: LogId,
{
    type Error = SqliteError;

    async fn associate(
        &self,
        topic: &Topic,
        author: &PublicKey,
        data_id: &S,
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
                .bind(topic.to_string())
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

    async fn remove(
        &self,
        topic: &Topic,
        author: &PublicKey,
        data_id: &S,
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
                .bind(topic.to_string())
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

    async fn resolve(&self, topic: &Topic) -> Result<HashMap<PublicKey, Vec<S>>, Self::Error> {
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
                .bind(topic.to_string())
                .fetch_all(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        let mut result: HashMap<PublicKey, Vec<S>> = HashMap::new();

        for (author, data_id) in data_ids {
            let author: PublicKey = author.parse().map_err(|_| {
                SqliteError::Decode("author".into(), crate::sqlite::DecodeError::FromStr)
            })?;

            let data_id = decode_cbor(&data_id[..])
                .map_err(|err| SqliteError::Decode("header".into(), err.into()))?;

            // All items in the returned data set will be unique due to the SQL UNIQUE constraint.
            result.entry(author).or_default().push(data_id);
        }

        Ok(result)
    }
}
