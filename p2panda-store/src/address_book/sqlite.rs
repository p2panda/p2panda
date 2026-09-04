// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Topic, VerifyingKey};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, query, query_as, query_scalar};

use crate::address_book::{AddressBookStore, NodeInfo};
use crate::sqlite::{SqliteError, SqliteStore};

impl<N> AddressBookStore<VerifyingKey, N> for SqliteStore
where
    N: NodeInfo<VerifyingKey> + Serialize + for<'de> Deserialize<'de> + Send + Unpin,
{
    type Error = SqliteError;

    async fn insert_node_info(&self, info: N) -> Result<bool, Self::Error> {
        let is_upsert = {
            let row = self
                .tx(async |tx| {
                    query_as::<_, (i32,)>("SELECT COUNT(*) FROM node_infos_v1 WHERE node_id = ?")
                        .bind(info.id().to_hex())
                        .fetch_one(&mut **tx)
                        .await
                        .map_err(SqliteError::Sqlite)
                })
                .await?;

            row.0 == 1
        };

        self.tx(async |tx| {
            query(
                "
                INSERT
                INTO
                    node_infos_v1 (
                        node_id,
                        node_info,
                        bootstrap,
                        stale
                    )
                VALUES
                    (?, ?, ?, ?)
                ON CONFLICT(node_id)
                DO UPDATE
                    SET
                        node_info = EXCLUDED.node_info,
                        bootstrap = EXCLUDED.bootstrap,
                        stale = EXCLUDED.stale
                ",
            )
            .bind(info.id().to_hex())
            .bind(NodeInfoEncode(&info))
            .bind(info.is_bootstrap())
            .bind(info.is_stale())
            .execute(&mut **tx)
            .await
            .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(!is_upsert)
    }

    async fn remove_node_info(&self, id: &VerifyingKey) -> Result<bool, Self::Error> {
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

        // Remove associated topics for removed nodes.
        self.tx(async |tx| {
            let mut query_builder = QueryBuilder::new(
                r#"
                DELETE FROM
                    topics2node_infos_v1
                WHERE
                    node_id IN ( "#,
            );

            {
                let mut separated = query_builder.separated(", ");
                for item in result.iter() {
                    separated.push_bind(&item.0);
                }
                separated.push_unseparated(") ");
            }

            query_builder
                .build()
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
        })
        .await?;

        Ok(result.len())
    }

    async fn node_info(&self, id: &VerifyingKey) -> Result<Option<N>, Self::Error> {
        query_as::<_, (NodeInfoDecode<N>,)>(
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
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteError::Sqlite)
        .map(|o| o.map(|(n,)| n.0))
    }

    async fn node_topics(&self, id: &VerifyingKey) -> Result<HashSet<Topic>, Self::Error> {
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
                    .map_err(|err| SqliteError::Decode("topic_id".to_string(), err.into()))
            })
            .collect()
    }

    async fn all_node_infos(&self) -> Result<Vec<N>, Self::Error> {
        query_as::<_, (NodeInfoDecode<N>,)>(
            "
                SELECT
                    node_info
                FROM
                    node_infos_v1
                WHERE
                    stale = FALSE
                ",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteError::Sqlite)
        .map(|v| v.into_iter().map(|(NodeInfoDecode(n),)| n).collect())
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
                    WHERE
                        stale = FALSE
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
                        AND stale = FALSE
                    ",
                )
                .fetch_one(pool)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        Ok(count as usize)
    }

    async fn selected_node_infos(&self, ids: &[VerifyingKey]) -> Result<Vec<N>, Self::Error> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                    node_info
                FROM
                    node_infos_v1
                WHERE
                    node_id IN (
            "#,
        );

        {
            let mut separated = query_builder.separated(", ");
            for id in ids.iter() {
                separated.push_bind(id.to_string());
            }
            separated.push_unseparated(" ) ");
        }

        query_builder
            .build_query_as::<(NodeInfoDecode<N>,)>()
            .fetch_all(&self.pool)
            .await
            .map_err(SqliteError::Sqlite)
            .map(|v| v.into_iter().map(|(NodeInfoDecode(n),)| n).collect())
    }

    async fn set_topics(
        &self,
        id: VerifyingKey,
        topics: HashSet<Topic>,
    ) -> Result<(), Self::Error> {
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
                .bind(topic.to_string())
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;
        }

        Ok(())
    }

    async fn node_infos_by_topics(&self, topics: &[Topic]) -> Result<Vec<N>, Self::Error> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                    node_infos_v1.node_info
                FROM
                    node_infos_v1
                LEFT JOIN topics2node_infos_v1
                    ON node_infos_v1.node_id = topics2node_infos_v1.node_id
                WHERE
                    topics2node_infos_v1.topic_id IN (
            "#,
        );

        {
            let mut separated = query_builder.separated(", ");
            for topic in topics {
                separated.push_bind(topic.to_string());
            }
            separated.push_unseparated(") ");
        }

        query_builder.push(
            r#"
                AND node_infos_v1.stale = FALSE
            GROUP BY
                node_infos_v1.node_id
            "#,
        );

        query_builder
            .build_query_as::<(NodeInfoDecode<N>,)>()
            .fetch_all(&self.pool)
            .await
            .map_err(SqliteError::Sqlite)
            .map(|v| v.into_iter().map(|(NodeInfoDecode(n),)| n).collect())
    }

    async fn random_node(&self) -> Result<Option<N>, Self::Error> {
        query_as::<_, (NodeInfoDecode<N>,)>(
            "
                SELECT
                    node_info
                FROM
                    node_infos_v1
                WHERE
                    stale = FALSE
                ORDER BY RANDOM()
                LIMIT 1
                ",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteError::Sqlite)
        .map(|o| o.map(|(n,)| n.0))
    }

    async fn random_bootstrap_node(&self) -> Result<Option<N>, Self::Error> {
        query_as::<_, (NodeInfoDecode<N>,)>(
            "
                SELECT
                    node_info
                FROM
                    node_infos_v1
                WHERE
                    bootstrap = TRUE
                    AND stale = FALSE
                ORDER BY RANDOM()
                LIMIT 1
            ",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteError::Sqlite)
        .map(|o| o.map(|(n,)| n.0))
    }
}

#[cfg(any(test, feature = "test_utils"))]
#[doc(hidden)]
impl SqliteStore {
    pub async fn set_last_changed(
        &self,
        id: &VerifyingKey,
        timestamp: u64,
    ) -> Result<(), SqliteError> {
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

struct NodeInfoDecode<N>(N);

impl<N> sqlx::Type<sqlx::Sqlite> for NodeInfoDecode<N>
where
    N: NodeInfo<VerifyingKey> + Serialize + for<'a> Deserialize<'a>,
{
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <Vec<u8> as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<N> sqlx::Decode<'_, sqlx::Sqlite> for NodeInfoDecode<N>
where
    N: NodeInfo<VerifyingKey> + Serialize + for<'a> Deserialize<'a>,
{
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'_>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;

        let cbor = decode_cbor(bytes)
            .map_err(|err| SqliteError::Decode("node_info".to_string(), err.into()))?;

        Ok(NodeInfoDecode(cbor))
    }
}

struct NodeInfoEncode<'a, N>(&'a N);

impl<'r, N> sqlx::Type<sqlx::Sqlite> for NodeInfoEncode<'r, N>
where
    N: NodeInfo<VerifyingKey> + Serialize + for<'a> Deserialize<'a>,
{
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <Vec<u8> as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'r, N> sqlx::Encode<'r, sqlx::Sqlite> for NodeInfoEncode<'r, N>
where
    N: NodeInfo<VerifyingKey> + Serialize + for<'a> Deserialize<'a>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let cbor_buf = encode_cbor(&self.0)
            .map_err(|err| SqliteError::Encode("node_info".to_string(), err))?;

        <Vec<u8> as sqlx::Encode<sqlx::Sqlite>>::encode(cbor_buf, buf)
    }
}
