-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS processor_v1 (
    namespace               TEXT            NOT NULL,
    id                      VARCHAR(32)     NOT NULL,
    event                   BLOB            NOT NULL,

    PRIMARY KEY (namespace, id)
);
