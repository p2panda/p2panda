// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// This template contains various types of valid operation.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case(crate::test_utils::fixtures::operation(
    Some(crate::test_utils::templates::defaults::fields()),
    None,
    crate::test_utils::constants::TEST_SCHEMA_ID.parse().unwrap(),
))]
#[allow(unused_qualifications)]
#[case::update_operation(
    crate::test_utils::fixtures::operation(
        Some(crate::test_utils::templates::defaults::fields()),
        Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
        crate::test_utils::constants::TEST_SCHEMA_ID.parse().unwrap(),
    )
)]
#[allow(unused_qualifications)]
#[case::delete_operation(
    crate::test_utils::fixtures::operation(
        None,
        Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
        crate::test_utils::constants::TEST_SCHEMA_ID.parse().unwrap()
    )
)]
#[allow(unused_qualifications)]
#[case::update_operation_many_previous(
    crate::test_utils::fixtures::operation(
        Some(crate::test_utils::templates::defaults::fields()),
        Some(DocumentViewId::new(&[
            crate::test_utils::fixtures::random_operation_id(),
            crate::test_utils::fixtures::random_operation_id(),
            crate::test_utils::fixtures::random_operation_id()
            ]).unwrap()),
        crate::test_utils::constants::TEST_SCHEMA_ID.parse().unwrap()
        )

)]
#[allow(unused_qualifications)]
#[case::delete_operation_many_previous(crate::test_utils::fixtures::operation(
    None,
    Some(DocumentViewId::new(&[
        crate::test_utils::fixtures::random_operation_id(),
        crate::test_utils::fixtures::random_operation_id(),
        crate::test_utils::fixtures::random_operation_id()
        ]).unwrap()),
        crate::test_utils::constants::TEST_SCHEMA_ID.parse().unwrap()
    )
)]
fn many_valid_operations(#[case] operation: Operation) {}

#[allow(unused_imports)]
pub(crate) use many_valid_operations;
