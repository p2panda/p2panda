use std::str::FromStr;

use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Extensions, Hash, Header, Operation, PublicKey, Signature};
use sqlx::FromRow;

use crate::logs::sqlite::SeqNum;

/// Database representation of the sum of all header and body byte size.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct ByteCount {
    total_header_size: String,
    total_payload_size: String,
}

impl From<ByteCount> for (u64, u64) {
    fn from(count: ByteCount) -> Self {
        (
            count.total_header_size.parse().unwrap(),
            count.total_payload_size.parse().unwrap(),
        )
    }
}

#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct LatestEntryRow {
    hash: String,
    seq_num: String,
}

impl From<LatestEntryRow> for (Hash, SeqNum) {
    fn from(row: LatestEntryRow) -> (Hash, SeqNum) {
        let hash = Hash::from_str(&row.hash).unwrap();
        let seq_num = row.seq_num.parse::<u64>().unwrap();

        (hash, seq_num)
    }
}

/// A single log height row as it is queried from the database.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct LogHeightRow {
    pub(crate) log_id: String,
    pub(crate) seq_num: String,
}

// TODO: Is it correct to use `u64` as the log id type here?
//
// No. We're waiting for upstream trait bound changes to `LogId`.
impl From<LogHeightRow> for (u64, SeqNum) {
    fn from(row: LogHeightRow) -> Self {
        (row.log_id.parse().unwrap(), row.seq_num.parse().unwrap())
    }
}
/// A single operation row as it is inserted in the database.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct OperationRow {
    hash: String,
    log_id: String,
    version: String,
    pub(crate) public_key: String,
    signature: String,
    payload_size: String,
    payload_hash: Option<String>,
    timestamp: String,
    pub(crate) seq_num: String,
    backlink: Option<String>,
    previous: String,
    extensions: Vec<u8>,
    pub(crate) body: Option<Vec<u8>>,
    pub(crate) header_bytes: Vec<u8>,
}

impl<E> From<OperationRow> for Header<E>
where
    E: Extensions,
{
    fn from(row: OperationRow) -> Self {
        let mut row_previous = row.previous;
        let mut previous = Vec::new();
        while !row_previous.is_empty() {
            let (hex, rest) = row_previous.split_at(32);
            // We assume database values are valid and therefore we're safe to unwrap.
            previous.push(Hash::from_str(hex).unwrap());
            row_previous = rest.to_string();
        }

        Header {
            version: row.version.parse::<u64>().unwrap(),
            public_key: PublicKey::from_str(&row.public_key).unwrap(),
            signature: Some(Signature::from_str(&row.signature).unwrap()),
            payload_size: row.payload_size.parse::<u64>().unwrap(),
            payload_hash: row.payload_hash.map(|hex| Hash::from_str(&hex).unwrap()),
            timestamp: row.timestamp.parse::<u64>().unwrap(),
            seq_num: row.seq_num.parse::<u64>().unwrap(),
            backlink: row.backlink.map(|hex| Hash::from_str(&hex).unwrap()),
            previous,
            extensions: decode_cbor(&row.extensions[..]).unwrap(),
        }
    }
}

impl<E> From<OperationRow> for Operation<E>
where
    E: Extensions,
{
    fn from(row: OperationRow) -> Self {
        let hash = Hash::from_str(&row.hash).unwrap();
        let header: Header<E> = row.clone().into();
        let body = row.body.map(|body| body.into());

        Self { hash, header, body }
    }
}
