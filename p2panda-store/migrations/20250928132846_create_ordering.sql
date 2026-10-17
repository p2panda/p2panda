-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE IF NOT EXISTS orderer_ready_v1 (
    namespace               TEXT            NOT NULL,
    id                      TEXT            NOT NULL,
    queue_index             INTEGER         NOT NULL,
    in_queue                BOOLEAN         NOT NULL    DEFAULT TRUE,

    PRIMARY KEY (namespace, id)
);

CREATE UNIQUE INDEX ux_orderer_ready_v1 ON orderer_ready_v1 (
    namespace,
    queue_index
);

-- This table resembles a data type like that:
--
-- {
--   [set_digest]: {
--     [id]: {
--       [child_id]: [parent_id, parent_id, ..],
--     }
--   }
-- }
--
-- 2 <-\
--      1   =>  insert(1, [2, 3])
-- 3 <-/
--
-- id: 2
-- child_id: 1
-- parent_id: 2
-- set_digest: Digest(1, 2, 3)
--
-- id: 2
-- child_id: 1
-- parent_id: 3
-- set_digest: Digest(1, 2, 3)
--
-- id: 3
-- child_id: 1
-- parent_id: 2
-- set_digest: Digest(1, 2, 3)
--
-- id: 3
-- child_id: 1
-- parent_id: 3
-- set_digest: Digest(1, 2, 3)
--
-- 2 <-\
--      4   =>  insert(4, [2])
--
-- id: 2
-- child_id: 4
-- parent_id: 2
-- set_digest: Digest(4, 2)
CREATE TABLE IF NOT EXISTS orderer_pending_v1 (
    namespace              TEXT            NOT NULL,
    id                     TEXT            NOT NULL,
    child_id               TEXT            NOT NULL,
    parent_id              TEXT            NOT NULL,
    set_digest             TEXT            NOT NULL
);

CREATE UNIQUE INDEX ux_orderer_pending_v1 ON orderer_pending_v1 (
    namespace,
    id,
    child_id,
    parent_id,
    set_digest
);

CREATE INDEX ix_orderer_pending_id_v1 ON orderer_pending_v1 (
    namespace,
    id
);

CREATE INDEX ix_orderer_pending_child_id_set_digest_v1 ON orderer_pending_v1 (
    namespace,
    child_id,
    set_digest
);
