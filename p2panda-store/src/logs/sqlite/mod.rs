// SPDX-License-Identifier: MIT OR Apache-2.0

mod models;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use p2panda_core::cbor::encode_cbor;
use p2panda_core::{Extensions, Hash, LogId, Operation, PublicKey, SeqNum};
use sqlx::{query, query_as};

use crate::logs::LogStore;
use crate::logs::sqlite::models::{LatestEntryRow, LogHeightRow, LogMetaRow};
use crate::operations::OperationRow;
use crate::sqlite::{SqliteError, SqliteStore};

const GET_LATEST_ENTRY: &str = "
    SELECT
        hash,
        seq_num
    FROM
        operations_v1
    WHERE
        public_key = ?
        AND log_id = ?
    ORDER BY
        CAST(seq_num AS NUMERIC) DESC LIMIT 1
";

impl<L, E> LogStore<Operation<E>, PublicKey, L, SeqNum, Hash> for SqliteStore
where
    E: Extensions,
    L: LogId,
{
    type Error = SqliteError;

    /// Retrieve the hash and sequence number of the latest entry in an author's log.
    async fn get_latest_entry(
        &self,
        author: &PublicKey,
        log_id: &L,
    ) -> Result<Option<(Hash, SeqNum)>, Self::Error> {
        if let Some(latest_entry) = query_as::<_, LatestEntryRow>(GET_LATEST_ENTRY)
            .bind(author.to_string())
            .bind(
                encode_cbor(&log_id)
                    .map_err(|err| SqliteError::Encode("log id".to_string(), err))?,
            )
            .fetch_optional(&self.pool)
            .await?
        {
            let hash_seq_num = latest_entry.try_into()?;

            Ok(Some(hash_seq_num))
        } else {
            Ok(None)
        }
    }

    /// Retrieve the hash and sequence number of the latest entry in an author's log.
    ///
    /// This variant of the method is intended to be used in situations where atomicity of database
    /// operations is needed. It requires a transaction context with an acquired permit.
    // TODO: In the future we may be able to remove this `_tx` variant of the query by instead
    // requiring that API users exlicitly handle transactions themselves.
    //
    // See: https://github.com/p2panda/p2panda/issues/1065
    async fn get_latest_entry_tx(
        &self,
        author: &PublicKey,
        log_id: &L,
    ) -> Result<Option<(Hash, SeqNum)>, Self::Error> {
        let result = self
            .tx(async |tx| {
                query_as::<_, LatestEntryRow>(GET_LATEST_ENTRY)
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

        if let Some(latest_entry) = result {
            let hash_seq_num = latest_entry.try_into()?;

            Ok(Some(hash_seq_num))
        } else {
            Ok(None)
        }
    }

    /// Retrieve the latest sequence number for a set of author's logs.
    async fn get_log_heights(
        &self,
        author: &PublicKey,
        logs: &[L],
    ) -> Result<Option<BTreeMap<L, SeqNum>>, Self::Error> {
        let mut encoded_log_ids = Vec::new();
        for log in logs {
            let encoded_log_id =
                encode_cbor(&log).map_err(|err| SqliteError::Encode("log id".to_string(), err))?;
            encoded_log_ids.push(encoded_log_id);
        }

        // This query formation approach is required since there is currently no
        // way to directly bind arrays as comma-separated lists in sqlx.
        let params = format!("?{}", ", ?".repeat(encoded_log_ids.len() - 1));
        let query_str = format!(
            "
            SELECT
                log_id,
                CAST(MAX(CAST(seq_num AS NUMERIC)) AS TEXT) as seq_num
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id IN ( {} )
            GROUP BY
                log_id
            ",
            params
        );

        let mut query = query_as::<_, LogHeightRow>(&query_str).bind(author.to_string());

        for log_id in encoded_log_ids {
            query = query.bind(log_id)
        }

        let log_heights_query = query.fetch_all(&self.pool).await?;

        let log_heights = if log_heights_query.is_empty() {
            None
        } else {
            let mut log_heights = BTreeMap::new();

            for row in log_heights_query {
                let (log_id, seq_num) = row.try_into()?;
                log_heights.insert(log_id, seq_num);
            }

            Some(log_heights)
        };

        Ok(log_heights)
    }

    /// Retrieve the count and total byte size of all operations in an author's log.
    async fn get_log_size(
        &self,
        author: &PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
    ) -> Result<Option<(u64, u64)>, Self::Error> {
        // We need to use an inclusive greater-than to ensure our
        // query includes the operation with sequence number 0.
        let after_operator = if after.is_none() { ">=" } else { ">" };
        let query_str = format!(
            "
            SELECT
                CAST(SUM(CAST(header_size AS NUMERIC)) AS TEXT) AS total_header_bytes,
                CAST(SUM(CAST(payload_size AS NUMERIC)) AS TEXT) AS total_payload_bytes,
                CAST(COUNT(*) AS TEXT) AS total_operation_count
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) {} CAST(? as NUMERIC)
                AND CAST(seq_num AS NUMERIC) <= CAST(? as NUMERIC)
            ",
            after_operator
        );

        let log_meta: Option<LogMetaRow> = query_as::<_, LogMetaRow>(&query_str)
            .bind(author.to_string())
            .bind(
                encode_cbor(&log_id)
                    .map_err(|err| SqliteError::Encode("log id".to_string(), err))?,
            )
            .bind(after.unwrap_or(0).to_string())
            .bind(until.unwrap_or(u64::MAX).to_string())
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = log_meta {
            let (total_header_bytes, total_payload_bytes, total_operation_count) =
                row.try_into()?;

            return Ok(Some((
                total_operation_count,
                total_header_bytes + total_payload_bytes,
            )));
        }

        Ok(None)
    }

    /// Retrieve log entries representing operations from an author's log.
    async fn get_log_entries(
        &self,
        author: &PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
    ) -> Result<Option<Vec<(Operation<E>, Vec<u8>)>>, Self::Error> {
        // We need to use an inclusive greater-than to ensure our
        // query includes the operation with sequence number 0.
        let after_operator = if after.is_none() { ">=" } else { ">" };

        let query_str = format!(
            "
            SELECT
                hash,
                header,
                body
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) {} CAST(? as NUMERIC)
                AND CAST(seq_num AS NUMERIC) <= CAST(? as NUMERIC)
            ORDER BY
                CAST(seq_num AS NUMERIC)
            ",
            after_operator
        );

        let operations = query_as::<_, OperationRow>(&query_str)
            .bind(author.to_string())
            .bind(
                encode_cbor(&log_id)
                    .map_err(|err| SqliteError::Encode("log id".to_string(), err))?,
            )
            .bind(after.unwrap_or(0).to_string())
            .bind(until.unwrap_or(u64::MAX).to_string())
            .fetch_all(&self.pool)
            .await?;

        let mut entries = Vec::new();
        for operation in operations {
            entries.push((operation.clone().try_into()?, operation.header))
        }

        if entries.is_empty() {
            Ok(None)
        } else {
            Ok(Some(entries))
        }
    }

    /// Prune entries from an author's log.
    ///
    /// Pruning involves deletion of the entry bodies (ie. payloads) from the database.
    async fn prune_entries(
        &self,
        author: &PublicKey,
        log_id: &L,
        until: &SeqNum,
    ) -> Result<u64, Self::Error> {
        let result = query(
            "
            DELETE
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) < CAST(? as NUMERIC)
            ",
        )
        .bind(author.to_string())
        .bind(encode_cbor(&log_id).map_err(|err| SqliteError::Encode("log id".to_string(), err))?)
        .bind(until.to_string())
        .execute(&self.pool)
        .await?;

        let pruned_entries_num = result.rows_affected();

        Ok(pruned_entries_num)
    }
}
