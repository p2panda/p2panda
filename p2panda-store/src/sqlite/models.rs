// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::From;
use std::str::FromStr;

use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Extensions, Hash, Header, PublicKey, RawOperation, Signature};
use sqlx::FromRow;

/// A single "raw" operation row as it is inserted in the database.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct RawOperationRow {
    hash: String,
    pub(crate) body: Option<Vec<u8>>,
    header_bytes: Vec<u8>,
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
    extensions: Option<Vec<u8>>,
    pub(crate) body: Option<Vec<u8>>,
    header_bytes: Vec<u8>,
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
            extensions: row
                .extensions
                .map(|extensions| decode_cbor(&extensions[..]).unwrap()),
        }
    }
}

impl From<RawOperationRow> for RawOperation {
    fn from(row: RawOperationRow) -> Self {
        (row.header_bytes, row.body)
    }
}

/// A single log height row as it is queried from the database.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
pub struct LogHeightRow {
    pub(crate) public_key: String,
    pub(crate) seq_num: String,
}

impl From<LogHeightRow> for (PublicKey, u64) {
    fn from(row: LogHeightRow) -> Self {
        (
            row.public_key.parse().unwrap(),
            row.seq_num.parse().unwrap(),
        )
    }
}
