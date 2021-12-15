// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods for generating some useful default values without any passed parameters. These are used
//! when composing test templates where default fixtures can't be injected in the usual way.

use crate::entry::Entry;
use crate::hash::Hash;
use crate::operation::{Operation, OperationValue};
use crate::test_utils::fixtures;
use crate::test_utils::utils::{DEFAULT_HASH, DEFAULT_SCHEMA_HASH};

/// The default hash as an option
pub fn some_hash() -> Option<Hash> {
    fixtures::some_hash(DEFAULT_HASH)
}

/// The default CREATE operation
pub fn create_operation() -> Operation {
    fixtures::create_operation(
        fixtures::schema(DEFAULT_SCHEMA_HASH),
        fixtures::fields(vec![(
            "message",
            OperationValue::Text("Hello!".to_string()),
        )]),
    )
}

/// The default UPDATE operation
pub fn update_operation() -> Operation {
    fixtures::update_operation(
        fixtures::schema(DEFAULT_SCHEMA_HASH),
        fixtures::hash(DEFAULT_HASH),
        fixtures::fields(vec![(
            "message",
            OperationValue::Text("Updated, hello!".to_string()),
        )]),
    )
}

/// The default DELETE operation
pub fn delete_operation() -> Operation {
    fixtures::delete_operation(
        fixtures::schema(DEFAULT_SCHEMA_HASH),
        fixtures::hash(DEFAULT_HASH),
    )
}

/// The default operation value
pub fn operation_value() -> OperationValue {
    OperationValue::Text("Hello!".to_string())
}

/// The default first entry
pub fn first_entry() -> Entry {
    fixtures::entry(create_operation(), fixtures::seq_num(1), None, None)
}

/// The default entry with only a backlink
pub fn entry_with_backlink() -> Entry {
    fixtures::entry(create_operation(), fixtures::seq_num(2), some_hash(), None)
}

/// The default entry with a backlink and skiplink
pub fn entry_with_backlink_and_skiplink() -> Entry {
    fixtures::entry(
        create_operation(),
        fixtures::seq_num(13),
        some_hash(),
        some_hash(),
    )
}

/// The default entry with a skiplink and no backlink
pub fn entry_with_only_a_skiplink() -> Entry {
    fixtures::entry(
        create_operation(),
        fixtures::seq_num(13),
        some_hash(),
        some_hash(),
    )
}
