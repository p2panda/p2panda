// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Hash, HashError, LogId, SeqNum};
use sqlx::FromRow;

use crate::sqlite::{DecodeError, SqliteError};

/// Database representation of the sum of all header and body byte size.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct LogMetaRow {
    pub total_header_bytes: String,
    pub total_payload_bytes: String,
    pub total_operation_count: String,
}

impl TryFrom<LogMetaRow> for (u64, u64, u64) {
    type Error = SqliteError;

    fn try_from(row: LogMetaRow) -> Result<Self, Self::Error> {
        let total_header_bytes: u64 = row
            .total_header_bytes
            .parse()
            .map_err(|_| SqliteError::Decode("header size".to_string(), DecodeError::FromStr))?;
        let total_payload_bytes: u64 = row
            .total_payload_bytes
            .parse()
            .map_err(|_| SqliteError::Decode("payload size".to_string(), DecodeError::FromStr))?;
        let total_operation_count: u64 = row.total_operation_count.parse().map_err(|_| {
            SqliteError::Decode("operation count".to_string(), DecodeError::FromStr)
        })?;

        Ok((
            total_header_bytes,
            total_payload_bytes,
            total_operation_count,
        ))
    }
}

/// Database representation of the hash and sequence number of the latest
/// operation in a log.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct LatestEntryRow {
    hash: String,
    seq_num: String,
}

impl TryFrom<LatestEntryRow> for (Hash, SeqNum) {
    type Error = SqliteError;

    fn try_from(row: LatestEntryRow) -> Result<Self, Self::Error> {
        let hash = row
            .hash
            .parse()
            .map_err(|err: HashError| SqliteError::Decode("hash".to_string(), err.into()))?;
        let seq_num = row
            .seq_num
            .parse()
            .map_err(|_| SqliteError::Decode("seq num".to_string(), DecodeError::FromStr))?;

        Ok((hash, seq_num))
    }
}

/// Database representation of a log ID and sequence number for a single operation.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct LogHeightRow {
    pub(crate) log_id: Vec<u8>,
    pub(crate) seq_num: String,
}

impl<L> TryFrom<LogHeightRow> for (L, SeqNum)
where
    L: LogId,
{
    type Error = SqliteError;

    fn try_from(row: LogHeightRow) -> Result<Self, Self::Error> {
        let log_id = decode_cbor(&row.log_id[..])
            .map_err(|err| SqliteError::Decode("log id".to_string(), err.into()))?;
        let seq_num = row
            .seq_num
            .parse()
            .map_err(|_| SqliteError::Decode("seq num".to_string(), DecodeError::FromStr))?;

        Ok((log_id, seq_num))
    }
}
