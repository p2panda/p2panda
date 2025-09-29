// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use p2panda_core::Hash;
use sqlx::{query, query_as};

use crate::orderer::OrdererStore;
#[cfg(any(test, feature = "test_utils"))]
use crate::orderer::traits::OrdererTestExt;
use crate::sqlite::{DecodeError, SqliteError, SqliteStore};

impl<'a, ID> OrdererStore<ID> for SqliteStore<'a>
where
    ID: Eq + Ord + StdHash + Display + FromStr,
{
    type Error = SqliteError;

    async fn mark_ready(&self, id: ID) -> Result<bool, Self::Error> {
        self.tx(async |tx| {
            let queue_index = {
                let last_index: (i64,) = query_as(
                    "
                    SELECT
                        MAX(queue_index)
                    FROM
                        orderer_ready_v1
                    ",
                )
                .fetch_one(&mut **tx)
                .await?;

                // This returns "0" (default) if no rows are in the database.
                last_index.0 + 1
            };

            let in_queue = true;

            // Ignore insertion when hash already exists (UNIQUE constraint).
            let result = query(
                "
                INSERT OR IGNORE
                INTO
                    orderer_ready_v1 (
                        id,
                        queue_index,
                        in_queue
                    )
                VALUES
                    (?, ?, ?)
                ",
            )
            .bind(id.to_string())
            .bind(queue_index)
            .bind(in_queue)
            .execute(&mut **tx)
            .await?;

            Ok(result.rows_affected() > 0)
        })
        .await
    }

    async fn mark_pending(
        &self,
        child_id: ID,
        mut parent_ids: Vec<ID>,
    ) -> Result<bool, Self::Error> {
        self.tx(async |tx| {
            let child_id = child_id.to_string();

            // Make hashing digest deterministic by sorting array first.
            parent_ids.sort();

            // Derive a hash from id (child) and all it's dependencies (parents).
            let set_digest = Hash::new({
                let mut buf: Vec<u8> = Vec::new();
                buf.extend_from_slice(child_id.as_bytes());
                for id in &parent_ids {
                    buf.extend_from_slice(id.to_string().as_bytes());
                }
                buf
            })
            .to_string();

            let mut insertion_occured = false;

            for id in &parent_ids {
                // Ignore items which are already marked as "ready".
                let is_ready = query(
                    "
                    SELECT
                        1
                    FROM
                        orderer_ready_v1
                    WHERE
                        id = ?
                    ",
                )
                .bind(id.to_string())
                .fetch_optional(&mut **tx)
                .await?
                .is_some();

                if is_ready {
                    continue;
                }

                let id = id.to_string();

                // Insert all dependencies for this non-ready dependency key with a set digest.
                for parent_id in &parent_ids {
                    let parent_id = parent_id.to_string();

                    let result = query(
                        "
                        INSERT OR IGNORE
                        INTO
                            orderer_pending_v1 (
                                id,
                                child_id,
                                parent_id,
                                set_digest
                            )
                        VALUES
                            (?, ?, ?, ?)
                        ",
                    )
                    .bind(&id)
                    .bind(&child_id)
                    .bind(&parent_id)
                    .bind(&set_digest)
                    .execute(&mut **tx)
                    .await?;

                    if result.rows_affected() > 0 {
                        insertion_occured = true;
                    }
                }
            }

            Ok(insertion_occured)
        })
        .await
    }

    async fn get_next_pending(
        &self,
        id: ID,
    ) -> Result<Option<HashSet<(ID, Vec<ID>)>>, Self::Error> {
        self.tx(async |tx| {
            // Find all unique (child_id, set_digest) combinations that depend on the given id.
            let sets: Vec<(String, String)> = query_as(
                "
                SELECT
                    DISTINCT child_id,
                    set_digest
                FROM
                    orderer_pending_v1
                WHERE
                    id = ?
                ",
            )
            .bind(id.to_string())
            .fetch_all(&mut **tx)
            .await?;

            if sets.is_empty() {
                return Ok(None);
            }

            let mut result = HashSet::new();

            // For each set, get the complete original dependency list.
            for (child_id, set_digest) in sets {
                let parent_ids: Vec<(String,)> = query_as(
                    "
                    SELECT
                        parent_id
                    FROM
                        orderer_pending_v1
                    WHERE
                        child_id = ?
                        AND set_digest = ?
                    ORDER BY
                        parent_id
                    ",
                )
                .bind(&child_id)
                .bind(&set_digest)
                .fetch_all(&mut **tx)
                .await?;

                let child_id = ID::from_str(&child_id)
                    .map_err(|_| SqliteError::Decode("child_id".into(), DecodeError::FromStr))?;

                let mut dependencies = Vec::new();
                for (parent_id,) in parent_ids {
                    let parent_id = ID::from_str(&parent_id).map_err(|_| {
                        SqliteError::Decode("parent_id".into(), DecodeError::FromStr)
                    })?;
                    dependencies.push(parent_id);
                }

                result.insert((child_id, dependencies));
            }

            Ok(Some(result))
        })
        .await
    }

    async fn take_next_ready(&self) -> Result<Option<ID>, Self::Error> {
        self.tx(async |tx| {
            let row: Option<(String,)> = query_as(
                "
                SELECT
                    id
                FROM
                    orderer_ready_v1
                WHERE
                    in_queue = TRUE
                ORDER BY
                    queue_index ASC
                LIMIT
                    1
                ",
            )
            .fetch_optional(&mut **tx)
            .await?;

            let Some((id_str,)) = row else {
                return Ok(None);
            };

            let id = ID::from_str(&id_str)
                .map_err(|_| SqliteError::Decode("id".into(), DecodeError::FromStr))?;

            query(
                "
                UPDATE
                    orderer_ready_v1
                SET
                    in_queue = FALSE
                WHERE
                    id = ?
                ",
            )
            .bind(&id_str)
            .execute(&mut **tx)
            .await?;

            Ok(Some(id))
        })
        .await
    }

    async fn remove_pending(&self, id: ID) -> Result<bool, Self::Error> {
        self.tx(async |tx| {
            let result = query(
                "
                DELETE FROM
                    orderer_pending_v1
                WHERE
                    id = ?
                ",
            )
            .bind(id.to_string())
            .execute(&mut **tx)
            .await?;

            Ok(result.rows_affected() > 0)
        })
        .await
    }

    async fn ready(&self, dependencies: &[ID]) -> Result<bool, Self::Error> {
        self.tx(async |tx| {
            let sql = format!(
                "
                SELECT
                    COUNT(id)
                FROM
                    orderer_ready_v1
                WHERE id IN ({})
                ",
                dependencies
                    .iter()
                    .map(|dep| format!("'{dep}'"))
                    .collect::<Vec<String>>()
                    .join(",")
            );

            let result: (i64,) = query_as(&sql).fetch_one(&mut **tx).await?;
            Ok(result.0 as usize == dependencies.len())
        })
        .await
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<'a> OrdererTestExt for SqliteStore<'a> {
    async fn ready_len(&self) -> usize {
        self.tx(async |tx| {
            let row: (i64,) = query_as(
                "
                SELECT
                    COUNT(id)
                FROM
                    orderer_ready_v1
                ",
            )
            .fetch_one(&mut **tx)
            .await?;
            Ok(row.0 as usize)
        })
        .await
        .unwrap()
    }

    async fn ready_queue_len(&self) -> usize {
        self.tx(async |tx| {
            let row: (i64,) = query_as(
                "
                SELECT
                    COUNT(id)
                FROM
                    orderer_ready_v1
                WHERE
                    in_queue = TRUE
                ",
            )
            .fetch_one(&mut **tx)
            .await?;
            Ok(row.0 as usize)
        })
        .await
        .unwrap()
    }

    async fn pending_len(&self) -> usize {
        self.tx(async |tx| {
            let row: (i64,) = query_as(
                "
                SELECT
                    COUNT(DISTINCT id)
                FROM
                    orderer_pending_v1
                ",
            )
            .fetch_one(&mut **tx)
            .await?;
            Ok(row.0 as usize)
        })
        .await
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use p2panda_core::Hash;

    use crate::orderer::OrdererStore;
    use crate::sqlite::SqliteStoreBuilder;

    #[tokio::test]
    async fn ready() {
        let store = SqliteStoreBuilder::new()
            .random_memory_url()
            .max_connections(1)
            .build()
            .await
            .unwrap();

        let hash_1 = Hash::new(b"tick");
        let hash_2 = Hash::new(b"trick");
        let hash_3 = Hash::new(b"track");

        let permit = store.begin().await.unwrap();

        // 1. Mark three items as "ready".
        assert!(store.mark_ready(hash_3).await.unwrap());
        assert!(store.mark_ready(hash_2).await.unwrap());

        // Should return false when trying to insert the same item again.
        assert!(!store.mark_ready(hash_2).await.unwrap());

        // 2. Should correctly tell us if dependencies have been met.
        assert!(store.ready(&[hash_2, hash_3]).await.unwrap());
        assert!(!store.ready(&[hash_1, hash_3]).await.unwrap());
        assert!(!store.ready(&[hash_1]).await.unwrap());

        // 3. Check if they come out in the queued-up order (FIFO) when calling "take_next_ready".
        assert_eq!(store.take_next_ready().await.unwrap(), Some(hash_3));

        // .. another item got inserted "mid-way".
        assert!(store.mark_ready(hash_1).await.unwrap());

        assert_eq!(store.take_next_ready().await.unwrap(), Some(hash_2));
        assert_eq!(store.take_next_ready().await.unwrap(), Some(hash_1));
        assert_eq!(
            OrdererStore::<Hash>::take_next_ready(&store).await.unwrap(),
            None
        );

        store.commit(permit).await.unwrap();
    }

    #[tokio::test]
    async fn pending() {
        let store = SqliteStoreBuilder::new()
            .random_memory_url()
            .max_connections(1)
            .build()
            .await
            .unwrap();

        let hash_1 = Hash::new(b"piff");
        let hash_2 = Hash::new(b"puff");
        let hash_3 = Hash::new(b"paff");
        let hash_4 = Hash::new(b"peff");

        let permit = store.begin().await.unwrap();

        // 1. Should correctly return true or false when insertion occured.
        assert!(
            store
                .mark_pending(hash_1, vec![hash_2, hash_3])
                .await
                .unwrap()
        );
        assert!(store.mark_pending(hash_1, vec![hash_3]).await.unwrap());
        assert!(!store.mark_pending(hash_1, vec![hash_3]).await.unwrap());
        assert!(
            store
                .mark_pending(hash_1, vec![hash_4, hash_3])
                .await
                .unwrap()
        );

        // 2. Return correct list of pending items.
        let pending = store.get_next_pending(hash_2).await.unwrap().unwrap();
        assert_eq!(pending.len(), 1);
        let (parent, deps) = pending.iter().next().unwrap();
        assert_eq!(*parent, hash_1);
        assert!(deps.contains(&hash_2));
        assert!(deps.contains(&hash_3));

        store.commit(permit).await.unwrap();
    }
}
