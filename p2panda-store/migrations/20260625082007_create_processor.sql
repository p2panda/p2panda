-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS processor_v1 (
    id                      VARCHAR(32)     NOT NULL   PRIMARY KEY,
    event                   BLOB            NOT NULL
);
