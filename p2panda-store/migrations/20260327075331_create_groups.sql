-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS groups_v1 (
    id                      VARCHAR(32)     NOT NULL   PRIMARY KEY,
    state                   BLOB            NOT NULL
);
