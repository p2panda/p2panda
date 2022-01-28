// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods for generating some useful default values without any passed parameters. These are used
//! when composing test templates where default fixtures can't be injected in the usual way.
use crate::entry::Entry;
use crate::hash::Hash;
use crate::operation::{Operation, OperationFields, OperationValue, OperationWithMeta};
use crate::test_utils::constants::{DEFAULT_HASH, DEFAULT_SCHEMA_HASH};
use crate::test_utils::fixtures;

/// The default hash.
pub fn hash() -> Hash {
    fixtures::hash(DEFAULT_HASH)
}

/// The default hash as an option.
pub fn some_hash() -> Option<Hash> {
    fixtures::some_hash(DEFAULT_HASH)
}

/// The default operation value.
pub fn operation_value() -> OperationValue {
    OperationValue::Text("Hello!".to_string())
}

/// The default operation fields.
pub fn fields() -> OperationFields {
    fixtures::fields(vec![("message", operation_value())])
}

/// The default CREATE operation.
pub fn create_operation() -> Operation {
    fixtures::create_operation(fixtures::schema(DEFAULT_SCHEMA_HASH), fields())
}

/// The default UPDATE operation.
pub fn update_operation() -> Operation {
    fixtures::update_operation(
        fixtures::schema(DEFAULT_SCHEMA_HASH),
        vec![fixtures::hash(DEFAULT_HASH)],
        fields(),
    )
}

/// The default DELETE operation.
pub fn delete_operation() -> Operation {
    fixtures::delete_operation(
        fixtures::schema(DEFAULT_SCHEMA_HASH),
        vec![fixtures::hash(DEFAULT_HASH)],
    )
}

/// The default CREATE meta-operation.
pub fn create_meta_operation() -> OperationWithMeta {
    let operation = create_operation();
    fixtures::meta_operation(
        fixtures::entry_signed_encoded(
            fixtures::entry(operation.clone(), fixtures::seq_num(1), None, None),
            fixtures::key_pair(fixtures::private_key()),
        ),
        fixtures::operation_encoded(operation),
    )
}

/// The default UPDATE meta-operation.
pub fn update_meta_operation() -> OperationWithMeta {
    let operation = update_operation();
    fixtures::meta_operation(
        fixtures::entry_signed_encoded(
            fixtures::entry(operation.clone(), fixtures::seq_num(2), some_hash(), None),
            fixtures::key_pair(fixtures::private_key()),
        ),
        fixtures::operation_encoded(operation),
    )
}

/// The default DELETE meta-operation.
pub fn delete_meta_operation() -> OperationWithMeta {
    let operation = delete_operation();
    fixtures::meta_operation(
        fixtures::entry_signed_encoded(
            fixtures::entry(operation.clone(), fixtures::seq_num(2), some_hash(), None),
            fixtures::key_pair(fixtures::private_key()),
        ),
        fixtures::operation_encoded(operation),
    )
}

/// The default first entry.
pub fn first_entry() -> Entry {
    fixtures::entry(create_operation(), fixtures::seq_num(1), None, None)
}

/// The default entry with only a backlink.
pub fn entry_with_backlink() -> Entry {
    fixtures::entry(create_operation(), fixtures::seq_num(2), some_hash(), None)
}

/// The default entry with a backlink and skiplink.
pub fn entry_with_backlink_and_skiplink() -> Entry {
    fixtures::entry(
        create_operation(),
        fixtures::seq_num(13),
        some_hash(),
        some_hash(),
    )
}

/// The default entry with a skiplink and no backlink.
pub fn entry_with_only_a_skiplink() -> Entry {
    fixtures::entry(
        create_operation(),
        fixtures::seq_num(13),
        some_hash(),
        some_hash(),
    )
}
