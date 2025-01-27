// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::From;
use std::str::FromStr;

use p2panda_core::{Extensions, Hash, Header, PublicKey, RawOperation, Signature};
use sqlx::FromRow;

use crate::sqlite::store::deserialize_extensions;

/// A struct representing a single operation row as it is inserted in the database.
#[derive(FromRow, Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct OperationRow {
    hash: String,
    log_id: String,
    version: String,
    public_key: String,
    signature: Option<String>,
    payload_size: String,
    payload_hash: Option<String>,
    timestamp: String,
    seq_num: String,
    backlink: Option<String>,
    previous: String,
    extensions: Option<Vec<u8>>,
    pub body: Option<Vec<u8>>,
    header_bytes: Vec<u8>,
}

impl<E> From<OperationRow> for Header<E>
where
    E: Extensions,
{
    fn from(operation_row: OperationRow) -> Self {
        let mut row_previous = operation_row.previous;
        let mut previous = Vec::new();
        while !row_previous.is_empty() {
            let (hex, rest) = row_previous.split_at(32);
            // We assume database values are valid and therefore we're safe to unwrap.
            previous.push(Hash::from_str(hex).unwrap());
            row_previous = rest.to_string();
        }

        Header {
            version: operation_row.version.parse::<u64>().unwrap(),
            public_key: PublicKey::from_str(&operation_row.public_key).unwrap(),
            signature: operation_row
                .signature
                .map(|hex| Signature::from_str(&hex).unwrap()),
            payload_size: operation_row.payload_size.parse::<u64>().unwrap(),
            payload_hash: operation_row
                .payload_hash
                .map(|hex| Hash::from_str(&hex).unwrap()),
            timestamp: operation_row.timestamp.parse::<u64>().unwrap(),
            seq_num: operation_row.seq_num.parse::<u64>().unwrap(),
            backlink: operation_row
                .backlink
                .map(|hex| Hash::from_str(&hex).unwrap()),
            previous,
            extensions: operation_row
                .extensions
                .map(|extensions| deserialize_extensions(extensions).unwrap()),
        }
    }
}

impl From<OperationRow> for RawOperation {
    fn from(operation_row: OperationRow) -> Self {
        (operation_row.header_bytes, operation_row.body)
    }
}
