-- SPDX-License-Identifier: MIT OR Apache-2.0

ALTER TABLE operations_v1
RENAME COLUMN public_key to verifying_key;
