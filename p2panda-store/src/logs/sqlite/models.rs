// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Hash, HashError, LogId, SeqNum};
use sqlx::FromRow;

use crate::sqlite::SqliteError;

/// Database representation of the sum of all header and body byte size.
#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct LogMetaRow {
    pub total_header_bytes: u32,
    pub total_payload_bytes: u32,
    pub total_operation_count: u32,
}

/// Database representation of the hash and sequence number of the latest
/// operation in a log.
#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct LatestEntryRow {
    hash: String,
    seq_num: SeqNum,
}

impl TryFrom<LatestEntryRow> for (Hash, SeqNum) {
    type Error = SqliteError;

    fn try_from(row: LatestEntryRow) -> Result<Self, Self::Error> {
        let hash = row
            .hash
            .parse()
            .map_err(|err: HashError| SqliteError::Decode("hash".to_string(), err.into()))?;

        Ok((hash, row.seq_num))
    }
}

/// Database representation of a log ID and sequence number for a single operation.
#[derive(Clone, Debug, PartialEq, Eq, FromRow)]
pub struct LogHeightRow {
    pub(crate) log_id: Vec<u8>,
    pub(crate) seq_num: SeqNum,
}

impl<L> TryFrom<LogHeightRow> for (L, SeqNum)
where
    L: LogId,
{
    type Error = SqliteError;

    fn try_from(row: LogHeightRow) -> Result<Self, Self::Error> {
        let log_id = decode_cbor(&row.log_id[..])
            .map_err(|err| SqliteError::Decode("log id".to_string(), err.into()))?;
        Ok((log_id, row.seq_num))
    }
}
