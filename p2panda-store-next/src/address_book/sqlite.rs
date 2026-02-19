// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt::Display;
use std::str::FromStr;
use std::time::Duration;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Hash, Topic};
use serde::{Deserialize, Serialize};
use sqlx::{query, query_as, query_scalar};

use crate::address_book::{AddressBookStore, NodeInfo};
use crate::sqlite::{SqliteError, SqliteStore};

impl<'a, N> AddressBookStore<Hash, N> for SqliteStore<'a>
where
    N: NodeInfo<Hash> + Serialize + for<'de> Deserialize<'de>,
{
    type Error = SqliteError;

    async fn insert_node_info(&self, info: N) -> Result<bool, Self::Error> {
        let result = self
            .tx(async |tx| {
                query(
                    "
                    INSERT
                    INTO
                        node_infos_v1 (
                            node_id,
                            node_info,
                            bootstrap
                        )
                    VALUES
                        (?, ?, ?)
                    ON CONFLICT(node_id)
                    DO UPDATE
                        SET
                            node_info = EXCLUDED.node_info,
                            bootstrap = EXCLUDED.bootstrap
                    ",
                )
                .bind(info.id().to_hex())
                .bind(
                    encode_cbor(&info)
                        .map_err(|err| SqliteError::Encode("node_info".to_string(), err))?,
                )
                .bind(info.is_bootstrap())
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn remove_node_info(&self, id: &Hash) -> Result<bool, Self::Error> {
        // Remove node's info.
        let result = self
            .tx(async |tx| {
                query(
                    "
                    DELETE FROM
                        node_infos_v1
                    WHERE
                        node_id = ?
                    ",
                )
                .bind(id.to_hex())
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        // Remove associated topics for this node.
        self.tx(async |tx| {
            query(
                "
                DELETE FROM
                    topics2node_infos_v1
                WHERE
                    node_id = ?
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

    async fn remove_older_than(&self, duration: Duration) -> Result<usize, Self::Error> {
        let result = self
            .tx(async |tx| {
                query_as::<_, (String,)>(
                    "
                    DELETE FROM
                        node_infos_v1
                    WHERE
                        updated_at < UNIXEPOCH() - ?
                    RETURNING
                        node_id
                    ",
                )
                .bind(duration.as_secs() as i64)
                .fetch_all(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        let node_ids: Vec<&String> = result.iter().map(|item| &item.0).collect();

        // Remove associated topics for removed nodes.
        self.tx(async |tx| {
            query(&format!(
                "
                DELETE FROM
                    topics2node_infos_v1
                WHERE
                    node_id IN ({})
                ",
                in_op_str(&node_ids)
            ))
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(node_ids.len())
    }

    async fn node_info(&self, id: &Hash) -> Result<Option<N>, Self::Error> {
        let result = self
            .execute(async |pool| {
                query_as::<_, (Vec<u8>,)>(
                    "
                    SELECT
                        node_info
                    FROM
                        node_infos_v1
                    WHERE
                        node_id = ?
                    ",
                )
                .bind(id.to_hex())
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        decode_node_info(result)
    }

    async fn node_topics(&self, id: &Hash) -> Result<HashSet<[u8; 32]>, Self::Error> {
        let result = self
            .execute(async |pool| {
                query_as::<_, (String,)>(
                    "
                    SELECT
                        topic_id
                    FROM
                        topics2node_infos_v1
                    WHERE
                        node_id = ?
                    ",
                )
                .bind(id.to_hex())
                .fetch_all(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        result
            .iter()
            .map(|item| {
                Topic::from_str(&item.0)
                    .map(|topic| topic.into())
                    .map_err(|err| SqliteError::Decode("topic_id".to_string(), err.into()))
            })
            .collect()
    }

    async fn all_node_infos(&self) -> Result<Vec<N>, Self::Error> {
        let result = self
            .execute(async |pool| {
                query_as::<_, (Vec<u8>,)>(
                    "
                    SELECT
                        node_info
                    FROM
                        node_infos_v1
                    ",
                )
                .fetch_all(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        decode_node_infos(result)
    }

    async fn all_nodes_len(&self) -> Result<usize, Self::Error> {
        let count: i64 = self
            .execute(async |pool| {
                query_scalar(
                    "
                    SELECT
                        COUNT(node_id)
                    FROM
                        node_infos_v1
                    ",
                )
                .fetch_one(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(count as usize)
    }

    async fn all_bootstrap_nodes_len(&self) -> Result<usize, Self::Error> {
        let count: i64 = self
            .execute(async |pool| {
                query_scalar(
                    "
                    SELECT
                        COUNT(node_id)
                    FROM
                        node_infos_v1
                    WHERE
                        bootstrap = TRUE
                    ",
                )
                .fetch_one(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(count as usize)
    }

    async fn selected_node_infos(&self, ids: &[Hash]) -> Result<Vec<N>, Self::Error> {
        let result = self
            .execute(async |pool| {
                query_as::<_, (Vec<u8>,)>(&format!(
                    "
                    SELECT
                        node_info
                    FROM
                        node_infos_v1
                    WHERE
                        node_id IN ({})
                    ",
                    in_op_str(ids)
                ))
                .fetch_all(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        decode_node_infos(result)
    }

    async fn set_topics(&self, id: Hash, topics: HashSet<[u8; 32]>) -> Result<(), Self::Error> {
        // Remove all previous topics set for this node id and replace it with new values. Both
        // updates will be executed inside the same atomic transaction.
        self.tx(async |tx| {
            query(
                "
                DELETE FROM
                    topics2node_infos_v1
                WHERE
                    node_id = ?
                ",
            )
            .bind(id.to_hex())
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        for topic in topics {
            self.tx(async |tx| {
                query(
                    "
                    INSERT OR IGNORE
                    INTO
                        topics2node_infos_v1 (
                            node_id,
                            topic_id
                        )
                    VALUES
                        (?, ?)
                    ",
                )
                .bind(id.to_hex())
                .bind(Topic::from(topic).to_string())
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;
        }

        Ok(())
    }

    async fn node_infos_by_topics(&self, topics: &[[u8; 32]]) -> Result<Vec<N>, Self::Error> {
        let topics: Vec<Topic> = topics.iter().map(|topic| Topic::from(*topic)).collect();

        let result = self
            .execute(async |pool| {
                query_as::<_, (Vec<u8>,)>(&format!(
                    "
                    SELECT
                        node_infos_v1.node_info
                    FROM
                        node_infos_v1
                    LEFT JOIN topics2node_infos_v1
                        ON node_infos_v1.node_id = topics2node_infos_v1.node_id
                    WHERE
                        topics2node_infos_v1.topic_id IN ({})
                    GROUP BY
                        node_infos_v1.node_id
                    ",
                    in_op_str(&topics)
                ))
                .fetch_all(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        decode_node_infos(result)
    }

    async fn random_node(&self) -> Result<Option<N>, Self::Error> {
        let result = self
            .execute(async |pool| {
                query_as::<_, (Vec<u8>,)>(
                    "
                    SELECT
                        node_info
                    FROM
                        node_infos_v1
                    ORDER BY RANDOM()
                    LIMIT 1
                    ",
                )
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        decode_node_info(result)
    }

    async fn random_bootstrap_node(&self) -> Result<Option<N>, Self::Error> {
        let result = self
            .execute(async |pool| {
                query_as::<_, (Vec<u8>,)>(
                    "
                    SELECT
                        node_info
                    FROM
                        node_infos_v1
                    WHERE
                        bootstrap = TRUE
                    ORDER BY RANDOM()
                    LIMIT 1
                    ",
                )
                .fetch_optional(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        decode_node_info(result)
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<'a> SqliteStore<'a> {
    pub async fn set_last_changed(&self, id: &Hash, timestamp: u64) -> Result<(), SqliteError> {
        self.tx(async |tx| {
            query(
                "
                UPDATE
                    node_infos_v1
                SET
                    updated_at = ?
                WHERE
                    node_id = ?
                ",
            )
            .bind(timestamp as i64)
            .bind(id.to_hex())
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(())
    }
}

/// Takes a list of items implementing `Display` to turn it into an SQL "IN" operator where each
/// item is represented as a string.
///
/// ```text
/// SELECT * FROM users
/// WHERE
///     id IN ('1a', '2b', '3c');
/// ```
fn in_op_str<T: Display>(list: &[T]) -> String {
    list.iter()
        .map(|item| format!("'{item}'"))
        .collect::<Vec<String>>()
        .join(",")
}

/// Deserialize multiple rows containing encoded node info.
fn decode_node_infos<N>(result: Vec<(Vec<u8>,)>) -> Result<Vec<N>, SqliteError>
where
    N: NodeInfo<Hash> + Serialize + for<'a> Deserialize<'a>,
{
    result
        .iter()
        .map(|item| {
            decode_cbor(&item.0[..])
                .map_err(|err| SqliteError::Decode("node_info".to_string(), err.into()))
        })
        .collect()
}

/// Deserialize single row maybe containing encoded node info.
fn decode_node_info<N>(result: Option<(Vec<u8>,)>) -> Result<Option<N>, SqliteError>
where
    N: NodeInfo<Hash> + Serialize + for<'a> Deserialize<'a>,
{
    match result {
        Some((bytes,)) => {
            Ok(Some(decode_cbor(&bytes[..]).map_err(|err| {
                SqliteError::Decode("node_info".to_string(), err.into())
            })?))
        }
        None => Ok(None),
    }
}
