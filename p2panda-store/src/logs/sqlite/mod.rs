// SPDX-License-Identifier: MIT OR Apache-2.0

mod models;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use async_stream::try_stream;
use p2panda_core::cbor::encode_cbor;
use p2panda_core::{AnyOperation, Hash, LogId, SeqNum, VerifyingKey};
use sqlx::{QueryBuilder, query, query_as, query_scalar};

use crate::logs::LogStore;
use crate::logs::sqlite::models::{LogHeightRow, LogMetaRow};
use crate::logs::traits::{LogEntry, LogStream};
use crate::operations::OperationRow;
use crate::sqlite::{SqliteError, SqliteStore};

const GET_LATEST_ENTRY: &str = "
    SELECT
        hash,
        header,
        body
    FROM
        operations_v1
    WHERE
        verifying_key = ?
        AND log_id = ?
    ORDER BY
        seq_num DESC LIMIT 1
";

const GET_LOG_ENTRY_HASHES: &str = "
    SELECT
        hash
    FROM
        operations_v1
    WHERE
        verifying_key = $1
        AND log_id = $2
        AND (
            ($3 == true AND seq_num >= $4)
            OR
            ($3 == false AND seq_num > $4)
        )
        AND seq_num <= $5
    ORDER BY
        seq_num
";

const GET_ENTRY: &str = "
    SELECT
        hash,
        header,
        body
    FROM
        operations_v1
    WHERE
        hash = ?
";

impl<L> LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash> for SqliteStore
where
    L: LogId + Send + 'static,
{
    type Error = SqliteError;

    /// Retrieve the latest entry in an author's log.
    async fn get_latest_entry_tx(
        &self,
        author: &VerifyingKey,
        log_id: &L,
    ) -> Result<Option<AnyOperation>, Self::Error> {
        let result = self
            .tx(async |tx| {
                query_as::<_, OperationRow>(GET_LATEST_ENTRY)
                    .bind(author.to_string())
                    .bind(
                        encode_cbor(&log_id)
                            .map_err(|err| SqliteError::Encode("log id".to_string(), err))?,
                    )
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(SqliteError::Sqlite)
            })
            .await?;

        if let Some(latest) = result {
            let operation = latest.try_into()?;

            Ok(Some(operation))
        } else {
            Ok(None)
        }
    }

    /// Retrieve the latest sequence number for a set of author's logs.
    async fn get_log_heights(
        &self,
        author: &VerifyingKey,
        logs: &[L],
    ) -> Result<Option<BTreeMap<L, SeqNum>>, Self::Error> {
        let mut query_builder = QueryBuilder::new(
            r#"
                SELECT
                    log_id,
                    MAX(seq_num) as seq_num
                FROM
                    operations_v1
                WHERE
                    verifying_key =
            "#,
        );

        query_builder.push_bind(author.to_string());
        query_builder.push(" AND log_id IN ( ");

        {
            let mut separated = query_builder.separated(", ");
            for log in logs {
                let encoded_log_id = encode_cbor(&log)
                    .map_err(|err| SqliteError::Encode("log id".to_string(), err))?;
                separated.push_bind(encoded_log_id);
            }

            separated.push_unseparated(") GROUP BY log_id");
        }

        let log_heights = query_builder
            .build_query_as::<LogHeightRow>()
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| row.try_into())
            .collect::<Result<BTreeMap<L, u32>, _>>()?;

        if log_heights.is_empty() {
            Ok(None)
        } else {
            Ok(Some(log_heights))
        }
    }

    /// Retrieve the count and total byte size of all operations in an author's log.
    async fn get_log_size(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
    ) -> Result<Option<(u32, u32)>, Self::Error> {
        let log_meta: Option<LogMetaRow> = query_as::<_, LogMetaRow>(
            "
            SELECT
                SUM(header_size) AS total_header_bytes,
                SUM(payload_size) AS total_payload_bytes,
                COUNT(*) AS total_operation_count
            FROM
                operations_v1
            WHERE
                verifying_key = $1
                AND log_id =$2
                AND (
                    ($3 == true AND seq_num >= $4)
                    OR
                    ($3 == false AND seq_num > $4)
                )
                AND seq_num <= $5
            ",
        )
        .bind(author.to_string())
        .bind(encode_cbor(&log_id).map_err(|err| SqliteError::Encode("log id".to_string(), err))?)
        // We need to use an inclusive greater-than to ensure our
        // query includes the operation with sequence number 0.
        .bind(after.is_none())
        .bind(after.unwrap_or(0).to_string())
        .bind(until.unwrap_or(SeqNum::MAX).to_string())
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = log_meta else {
            return Ok(None);
        };

        Ok(Some((
            row.total_operation_count,
            row.total_header_bytes + row.total_payload_bytes,
        )))
    }

    /// Stream all entries in a log after an optional starting point keeping only keep one entry
    /// in memory at a time.
    fn log_entries(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
    ) -> Result<LogStream<AnyOperation, L, Self::Error>, Self::Error> {
        let author = author.to_string();
        let log_id = log_id.clone();
        let log_id_bytes =
            encode_cbor(&log_id).map_err(|err| SqliteError::Encode("log id".to_string(), err))?;
        let after_is_none = after.is_none();
        let after_value = after.unwrap_or(0).to_string();
        let until_value = until.unwrap_or(SeqNum::MAX).to_string();
        let pool = self.pool.clone();

        let stream = try_stream! {
            // First query all ids for entries in this log range.
            let operation_ids: Vec<String> = query_scalar(GET_LOG_ENTRY_HASHES)
                .bind(author)
                .bind(log_id_bytes)
                .bind(after_is_none)
                .bind(after_value)
                .bind(until_value)
                .fetch_all(&pool)
                .await?;

            // Iterate over ids and retrieve each entry.
            for id in operation_ids {
                let row = query_as::<_, OperationRow>(GET_ENTRY)
                    .bind(id)
                    .fetch_optional(&pool)
                    .await?;

                // Ignore if the entry is missing now, it could have been deleted or pruned since
                // the ids were retrieved.
                let Some(row) = row else {
                    continue;
                };

                let header = row.header.clone();
                let operation: AnyOperation = row.try_into()?;

                yield LogEntry { entry: operation, log_id: log_id.clone(), bytes: header };
            }
        };

        Ok(Box::pin(stream))
    }

    /// Prune entries from an author's log.
    ///
    /// Pruning involves deletion of the entry bodies (ie. payloads) from the database.
    async fn prune_entries_tx(
        &self,
        author: &VerifyingKey,
        log_id: &L,
        until: &SeqNum,
    ) -> Result<u64, Self::Error> {
        let result = self
            .tx(async |tx| {
                query(
                    "
                    DELETE FROM
                        operations_v1
                    WHERE
                        verifying_key = ?
                        AND log_id = ?
                        AND seq_num < ?
                    ",
                )
                .bind(author.to_string())
                .bind(
                    encode_cbor(&log_id)
                        .map_err(|err| SqliteError::Encode("log id".to_string(), err))?,
                )
                .bind(until.to_string())
                .execute(&mut **tx)
                .await
                .map_err(SqliteError::Sqlite)
            })
            .await?;

        let pruned_entries_num = result.rows_affected();
        Ok(pruned_entries_num)
    }
}
