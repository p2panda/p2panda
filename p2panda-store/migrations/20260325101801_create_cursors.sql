-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS cursors_v1 (
    name                    TEXT            NOT NULL    PRIMARY KEY,
    cursor                  BLOB            NOT NULL
);
