// SPDX-License-Identifier: MIT OR Apache-2.0

mod models;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash as StdHash, Hasher};

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Extensions, Hash, LogId, Operation, PublicKey};
use sqlx::{query, query_as};

use crate::logs::LogStore;
use crate::logs::sqlite::models::{ByteCount, LatestEntryRow, LogHeightRow};
use crate::operations::OperationRow;
use crate::sqlite::{SqliteError, SqliteStore};

pub type SeqNum = u64;

// TODO: We have this in a couple of places in the codebase; can we centralise?
fn calculate_hash<T: StdHash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

impl<'a, L, E> LogStore<Operation<E>, PublicKey, L, SeqNum, Hash> for SqliteStore<'a>
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
        if let Some(latest_entry) = query_as::<_, LatestEntryRow>(
            "
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
            ",
        )
        .bind(author.to_string())
        .bind(encode_cbor(&log_id).map_err(|err| SqliteError::Encode("log id".to_string(), err))?)
        .fetch_optional(&self.pool)
        .await?
        {
            let hash_seq_num = latest_entry.into();

            Ok(Some(hash_seq_num))
        } else {
            Ok(None)
        }
    }

    async fn get_log_heights(
        &self,
        author: &PublicKey,
        logs: &[L],
    ) -> Result<Option<HashMap<L, SeqNum>>, Self::Error> {
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
                AND log_id IN ( { } )
            GROUP BY
                public_key
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
            let mut log_heights = HashMap::new();

            for row in log_heights_query {
                log_heights.insert(
                    decode_cbor(&row.log_id[..])
                        .map_err(|err| SqliteError::Decode("log id".to_string(), err.into()))?,
                    row.seq_num.parse::<u64>().unwrap(),
                );
            }

            Some(log_heights)
        };

        Ok(log_heights)
    }

    async fn get_log_size(
        &self,
        author: &PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
    ) -> Result<Option<(u64, u64)>, Self::Error> {
        let rows = query_as::<_, ByteCount>(
            "
            SELECT
                CAST(SUM(CAST(header_size AS NUMERIC)) AS TEXT) AS total_header_size,
                CAST(SUM(CAST(payload_size AS NUMERIC)) AS TEXT) AS total_payload_size
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) > CAST(? as NUMERIC)
                AND CAST(seq_num AS NUMERIC) <= CAST(? as NUMERIC)
            ",
        )
        .bind(author.to_string())
        .bind(calculate_hash(log_id).to_string())
        .bind(after.unwrap_or(0).to_string())
        .bind(until.unwrap_or(u64::MAX).to_string())
        .fetch_one(&self.pool)
        .await?;

        // TODO: We need to update the query to be able to return total header_size, total
        // payload_size _and_ total number of operations whose size was summed.

        /*
        // Total number of operations returned from the query.
        let operations: u64 = rows
            .len()
            .try_into()
            // TODO: Error handling; need to map to an appropriate SqliteError type.
            .unwrap();

        let mut bytes = 0;
        for row in rows {
            let (header_size, payload_size) = row.into();
            bytes += header_size;
            bytes += payload_size;
        }

        if bytes == 0 {
            Ok(None)
        } else {
            Ok(Some((operations, bytes)))
        }
        */

        todo!()
    }

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
