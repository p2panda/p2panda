// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods for generating some useful default values without any passed parameters. These are used
//! when composing test templates where default fixtures can't be injected in the usual way.
use crate::entry::Entry;
use crate::operation::{Operation, OperationFields, OperationWithMeta};
use crate::test_utils::constants::{default_fields, DEFAULT_HASH, TEST_SCHEMA_ID};
use crate::test_utils::fixtures;

pub fn fields() -> OperationFields {
    fixtures::operation_fields(default_fields())
}

/// The default CREATE operation.
pub fn create_operation() -> Operation {
    fixtures::operation(Some(fields()), None, TEST_SCHEMA_ID.parse().unwrap())
}

/// The default UPDATE operation.
pub fn update_operation() -> Operation {
    fixtures::operation(
        Some(fields()),
        Some(vec![fixtures::operation_id(DEFAULT_HASH)]),
        TEST_SCHEMA_ID.parse().unwrap(),
    )
}

/// The default DELETE operation.
pub fn delete_operation() -> Operation {
    fixtures::operation(
        None,
        Some(vec![fixtures::operation_id(DEFAULT_HASH)]),
        TEST_SCHEMA_ID.parse().unwrap(),
    )
}

/// The default CREATE meta-operation.
pub fn create_meta_operation() -> OperationWithMeta {
    let operation = create_operation();
    fixtures::meta_operation(
        fixtures::entry_signed_encoded(
            fixtures::entry(
                operation.clone(),
                fixtures::seq_num(1),
                None,
                None,
                fixtures::log_id(1),
            ),
            fixtures::key_pair(&fixtures::private_key()),
        ),
        fixtures::operation_encoded(operation),
    )
}

/// The default UPDATE meta-operation.
pub fn update_meta_operation() -> OperationWithMeta {
    let operation = update_operation();
    fixtures::meta_operation(
        fixtures::entry_signed_encoded(
            fixtures::entry(
                operation.clone(),
                fixtures::seq_num(2),
                fixtures::some_hash(DEFAULT_HASH),
                None,
                fixtures::log_id(1),
            ),
            fixtures::key_pair(&fixtures::private_key()),
        ),
        fixtures::operation_encoded(operation),
    )
}

/// The default DELETE meta-operation.
pub fn delete_meta_operation() -> OperationWithMeta {
    let operation = delete_operation();
    fixtures::meta_operation(
        fixtures::entry_signed_encoded(
            fixtures::entry(
                operation.clone(),
                fixtures::seq_num(2),
                fixtures::some_hash(DEFAULT_HASH),
                None,
                fixtures::log_id(1),
            ),
            fixtures::key_pair(&fixtures::private_key()),
        ),
        fixtures::operation_encoded(operation),
    )
}

/// The default first entry.
pub fn first_entry() -> Entry {
    fixtures::entry(
        create_operation(),
        fixtures::seq_num(1),
        None,
        None,
        fixtures::log_id(1),
    )
}

/// The default entry with only a backlink.
pub fn entry_with_backlink() -> Entry {
    fixtures::entry(
        create_operation(),
        fixtures::seq_num(2),
        fixtures::some_hash(DEFAULT_HASH),
        None,
        fixtures::log_id(1),
    )
}

/// The default entry with a backlink and skiplink.
pub fn entry_with_backlink_and_skiplink() -> Entry {
    fixtures::entry(
        create_operation(),
        fixtures::seq_num(13),
        fixtures::some_hash(DEFAULT_HASH),
        fixtures::some_hash(DEFAULT_HASH),
        fixtures::log_id(1),
    )
}

/// The default entry with a skiplink and no backlink.
pub fn entry_with_only_a_skiplink() -> Entry {
    fixtures::entry(
        create_operation(),
        fixtures::seq_num(13),
        fixtures::some_hash(DEFAULT_HASH),
        fixtures::some_hash(DEFAULT_HASH),
        fixtures::log_id(1),
    )
}
