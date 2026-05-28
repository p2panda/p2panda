-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS operations_v1 (
    hash                    VARCHAR(32)     NOT NULL    PRIMARY KEY,
    log_id                  BLOB            NOT NULL,
    version                 INTEGER         NOT NULL,
    public_key              VARCHAR(32)     NOT NULL,
    signature               VARCHAR(64)     NOT NULL,
    payload_size            INTEGER         NOT NULL,
    payload_hash            VARCHAR(32)     NULL,
    timestamp               TEXT            NOT NULL,
    seq_num                 INTEGER         NOT NULL,
    header                  BLOB            NOT NULL,
    header_size             INTEGER         NOT NULL,
    body                    BLOB            NULL
);
