// SPDX-License-Identifier: MIT OR Apache-2.0

mod models;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash as StdHash, Hasher};

use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Extensions, Hash, LogId, Operation, PublicKey};
use sqlx::{query, query_as};

use crate::logs::sqlite::models::{ByteCount, LatestEntryRow, LogHeightRow, OperationRow};
use crate::logs::LogStore;
use crate::sqlite::{SqliteError, SqliteStore};

pub type SeqNum = u64;

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
                seq_num,
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

        let operations = query_as::<_, LogHeightRow>(
            "
            SELECT
                log_id,
                CAST(MAX(CAST(seq_num AS NUMERIC)) AS TEXT) as seq_num
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id IN ?
            GROUP BY
                public_key
            ",
        )
        .bind(author.to_string())
        .bind(&encoded_log_ids[..])
        .fetch_all(&self.pool)
        .await?;

        let log_heights = if operations.is_empty() {
            None
        } else {
            Some(
                operations
                    .iter()
                    .map(|row| {
                        (
                            decode_cbor(row.log_id.to_string()).unwrap(),
                            row.seq_num.parse::<u64>().unwrap(),
                        )
                    })
                    .collect(),
            )
        };
        //.for_each(|row| log_heights.insert(row.log_id, row.seq_num).un);

        /*
        let log_heights: Vec<(PublicKey, u64)> = operations
            .into_iter()
            .map(|operation| operation.into())
            .collect();
        */

        Ok(log_heights)
    }

    async fn get_log_size(
        &self,
        author: &PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
    ) -> Result<Option<(u64, u64)>, Self::Error> {
        let byte_count = query_as::<_, ByteCount>(
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
        .bind(until.unwrap_or(u64::max_value()).to_string())
        .fetch_one(&self.pool)
        .await?;

        let (header_bytes, body_bytes) = byte_count.into();

        if header_bytes + body_bytes == 0 {
            Ok(None)
        } else {
            Ok(Some((header_bytes, body_bytes)))
        }
    }

    async fn get_log_entries(
        &self,
        author: &PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
        until: Option<SeqNum>,
    ) -> Result<Option<Vec<(Operation<E>, Vec<u8>)>>, Self::Error> {
        let operations = query_as::<_, OperationRow>(
            "
            SELECT
                hash,
                log_id,
                version,
                public_key,
                signature,
                payload_size,
                payload_hash,
                timestamp,
                seq_num,
                backlink,
                previous,
                extensions,
                body,
                header_bytes
            FROM
                operations_v1
            WHERE
                public_key = ?
                AND log_id = ?
                AND CAST(seq_num AS NUMERIC) > CAST(? as NUMERIC)
                AND CAST(seq_num AS NUMERIC) <= CAST(? as NUMERIC)
            ORDER BY
                CAST(seq_num AS NUMERIC)
            ",
        )
        .bind(author.to_string())
        .bind(encode_cbor(&log_id).map_err(|err| SqliteError::Encode("log id".to_string(), err))?)
        .bind(after.unwrap_or(0).to_string())
        .bind(until.unwrap_or(u64::max_value()).to_string())
        .fetch_all(&self.pool)
        .await?;

        let entries: Vec<(Operation<E>, Vec<u8>)> = operations
            .into_iter()
            .map(|operation| (operation.clone().into(), operation.header_bytes))
            .collect();

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

#[cfg(test)]
mod tests {
    use p2panda_core::test_utils::TestLog;
    use p2panda_core::{Body, Hash, Header, PrivateKey};

    use crate::logs::LogStore;
    use crate::operations::OperationStore;
    use crate::sqlite::{SqliteStore, SqliteStoreBuilder};

    fn create_operation(
        private_key: &PrivateKey,
        body: &Body,
        seq_num: u64,
        timestamp: u64,
        backlink: Option<Hash>,
    ) -> (Hash, Header<()>, Vec<u8>) {
        let mut header = Header {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp,
            seq_num,
            backlink,
            previous: vec![],
            extensions: (),
        };
        header.sign(private_key);
        let header_bytes = header.to_bytes();
        (header.hash(), header, header_bytes)
    }

    #[tokio::test]
    async fn get_latest_entry() {
        let mut store = SqliteStoreBuilder::new()
            .run_default_migrations(false)
            .random_memory_url()
            .build()
            .await
            .unwrap();

        let log = TestLog::new();

        let operation_1 = log.operation(b"first", ());
        let operation_2 = log.operation(b"second", ());

        assert!(store
            .insert_operation(&operation_1.hash.clone(), operation_1.clone(), log.id())
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn get_log_heights() {
        todo!()
    }

    #[tokio::test]
    async fn get_log_size() {
        todo!()
    }

    #[tokio::test]
    async fn get_log_entries() {
        todo!()
    }

    #[tokio::test]
    async fn prune_entries() {
        todo!()
    }
}
