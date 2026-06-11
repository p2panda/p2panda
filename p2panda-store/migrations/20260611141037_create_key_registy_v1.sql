-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS key_registry_v1 (
    id                      TEXT            NOT NULL    PRIMARY KEY,
    state                   BLOB            NOT NULL
);
