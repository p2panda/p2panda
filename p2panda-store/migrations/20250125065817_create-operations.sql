-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS operations_v1 (
    hash                    TEXT            NOT NULL    PRIMARY KEY,
    log_id                  TEXT            NOT NULL,
    version                 TEXT            NOT NULL,
    public_key              TEXT            NOT NULL,
    signature               TEXT            NOT NULL,
    payload_size            TEXT            NOT NULL,
    payload_hash            TEXT            NULL,
    timestamp               TEXT            NOT NULL,
    seq_num                 TEXT            NOT NULL,
    backlink                TEXT            NULL,
    previous                TEXT            NOT NULL,
    extensions              BLOB            NULL,
    body                    BLOB            NULL,
    header_bytes            TEXT            NOT NULL
);
