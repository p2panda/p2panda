// SPDX-License-Identifier: MIT OR Apache-2.0

use sqlx::FromRow;

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
    body: Option<Vec<u8>>,
    header_bytes: Vec<u8>,
}
