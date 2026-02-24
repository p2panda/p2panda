-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS topics_v1 (
    topic                   TEXT            NOT NULL,
    author                  VARCHAR(64)     NOT NULL,
    data_id                 BLOB            NOT NULL,

    UNIQUE (topic, author, data_id)
);
