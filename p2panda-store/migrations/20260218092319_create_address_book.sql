-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS node_infos_v1 (
    node_id                 VARCHAR(64)     NOT NULL    PRIMARY KEY,
    node_info               BLOB            NOT NULL,
    bootstrap               BOOLEAN         NOT NULL    DEFAULT 0,
    created_at              INTEGER         NOT NULL    DEFAULT(UNIXEPOCH()),
    updated_at              INTEGER         NOT NULL    DEFAULT(UNIXEPOCH())
);

-- Automatically set updated_at to the current timestamp when node_info changes.
CREATE TRIGGER tg_node_infos_updated_at_v1 UPDATE OF node_info ON node_infos_v1
    BEGIN
        UPDATE node_infos_v1
        SET
            updated_at = UNIXEPOCH()
        WHERE
            node_id = OLD.node_id;
    END;

CREATE TABLE IF NOT EXISTS topics2node_infos_v1 (
    node_id                 VARCHAR(64)     NOT NULL,
    topic_id                VARCHAR(64)     NOT NULL
);

CREATE UNIQUE INDEX ux_topics2node_infos_v1 ON topics2node_infos_v1 (
    node_id,
    topic_id
);
