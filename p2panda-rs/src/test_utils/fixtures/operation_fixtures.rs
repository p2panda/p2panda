// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::document::DocumentViewId;
use crate::operation::{OperationFields, OperationId, OperationValue};
use crate::test_utils::constants::{test_fields, HASH};
use crate::test_utils::fixtures::random_hash;

/// Returns constant testing operation id.
#[fixture]
pub fn operation_id(#[default(HASH)] hash_str: &str) -> OperationId {
    hash_str.parse().unwrap()
}

/// Generates a new random operation id.
#[fixture]
pub fn random_operation_id() -> OperationId {
    random_hash().into()
}

/// Returns constant operation value.
#[fixture]
pub fn operation_value() -> OperationValue {
    OperationValue::String("Hello!".to_string())
}

/// Returns document view id of any number of operations containing random hashes.
#[fixture]
pub fn random_previous_operations(#[default(1)] num: u32) -> DocumentViewId {
    let mut previous: Vec<OperationId> = Vec::new();

    for _ in 0..num {
        previous.push(random_hash().into())
    }

    // Make sure the random hashes are sorted, otherwise validation will fail when creating the
    // document view id
    previous.sort();

    DocumentViewId::new(&previous)
}

/// Returns operation fields populated with test values.
#[fixture]
pub fn operation_fields(
    #[default(test_fields())] fields: Vec<(&'static str, OperationValue)>,
) -> Vec<(&'static str, OperationValue)> {
    fields
}
