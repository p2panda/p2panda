// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// This template contains various types of valid operation.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case(crate::test_utils::fixtures::operation(
    Some(crate::test_utils::fixtures::operation_fields(
        crate::test_utils::constants::default_fields()
    )),
    None,
    None,
))]
#[allow(unused_qualifications)]
#[case::update_operation(
    crate::test_utils::fixtures::operation(
        Some(crate::test_utils::fixtures::operation_fields(crate::test_utils::constants::default_fields())),
        Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
        None,
    )
)]
#[allow(unused_qualifications)]
#[case::delete_operation(
    crate::test_utils::fixtures::operation(
        None,
        Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
        None
    )
)]
#[allow(unused_qualifications)]
#[case::update_operation_many_previous(
    crate::test_utils::fixtures::operation(
        Some(crate::test_utils::fixtures::operation_fields(crate::test_utils::constants::default_fields())),
        Some(DocumentViewId::new(&[
            crate::test_utils::fixtures::random_operation_id(),
            crate::test_utils::fixtures::random_operation_id(),
            crate::test_utils::fixtures::random_operation_id()
            ]).unwrap()),
        None
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
        None
    )
)]
fn many_valid_operations(#[case] operation: Operation) {}

/// This template contains various types of valid meta-operation.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_meta_operation(crate::test_utils::fixtures::operation_with_meta(
    Some(crate::test_utils::fixtures::operation_fields(default_fields())),
    None,
    None,
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::update_meta_operation(crate::test_utils::fixtures::operation_with_meta(
    Some(crate::test_utils::fixtures::operation_fields(default_fields())),
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    None,
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::delete_meta_operation(crate::test_utils::fixtures::operation_with_meta(
    None,
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    None,
    None,
    None
))]
fn various_operation_with_meta(#[case] operation: VerifiedOperation) {}

/// This template contains examples of all structs which implement the `AsOperation` trait.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_operation(crate::test_utils::fixtures::operation(
    Some(crate::test_utils::fixtures::operation_fields(default_fields())),
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::update_operation(crate::test_utils::fixtures::operation(
    Some(crate::test_utils::fixtures::operation_fields(default_fields())),
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    None
))]
#[allow(unused_qualifications)]
#[case::delete_operation(crate::test_utils::fixtures::operation(None, Some(
    crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    None
))]
#[allow(unused_qualifications)]
#[case::create_meta_operation(crate::test_utils::fixtures::operation_with_meta(
    Some(crate::test_utils::fixtures::operation_fields(default_fields())),
    None,
    None,
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::update_meta_operation(crate::test_utils::fixtures::operation_with_meta(
    Some(crate::test_utils::fixtures::operation_fields(default_fields())),
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    None,
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::delete_meta_operation(crate::test_utils::fixtures::operation_with_meta(
    None,
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    None,
    None,
    None
))]
fn implements_as_operation(#[case] operation: impl AsOperation) {}

#[allow(unused_imports)]
pub(crate) use implements_as_operation;
#[allow(unused_imports)]
pub(crate) use many_valid_operations;
#[allow(unused_imports)]
pub(crate) use various_operation_with_meta;
