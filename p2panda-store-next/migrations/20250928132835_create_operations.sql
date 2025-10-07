-- SPDX-License-Identifier: MIT OR Apache-2.0

-- NOTE: SQLite doesn't support u64 values, we're storing large integers as
-- defined in the p2panda specification as TEXT.
CREATE TABLE IF NOT EXISTS operations_v1 (
    hash                    TEXT            NOT NULL    PRIMARY KEY,
    version                 TEXT            NOT NULL,
    public_key              TEXT            NOT NULL,
    signature               TEXT            NOT NULL,
    payload_size            TEXT            NOT NULL,
    payload_hash            TEXT            NULL,
    timestamp               TEXT            NOT NULL,
    header                  BLOB            NOT NULL,
    body                    BLOB            NULL,
    extensions              BLOB            NULL
);
