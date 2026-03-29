-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS groups_v1 (
    id                      BLOB            NOT NULL   PRIMARY KEY,
    state                   BLOB            NOT NULL
);
