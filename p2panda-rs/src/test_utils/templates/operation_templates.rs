// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// This template contains various types of valid operation.
#[template]
#[export]
#[rstest]
#[allow(unused_qualifications)]
#[case($crate::test_utils::fixtures::operation(
    Some($crate::test_utils::fixtures::operation_fields(
        $crate::test_utils::constants::test_fields()
    )),
    None,
    None,
))]
#[allow(unused_qualifications)]
#[case::update_operation(
    $crate::test_utils::fixtures::operation(
        Some($crate::test_utils::fixtures::operation_fields($crate::test_utils::constants::test_fields())),
        Some($crate::test_utils::constants::HASH.parse().unwrap()),
        None,
    )
)]
#[allow(unused_qualifications)]
#[case::delete_operation(
    $crate::test_utils::fixtures::operation(
        None,
        Some($crate::test_utils::constants::HASH.parse().unwrap()),
        None
    )
)]
#[allow(unused_qualifications)]
#[case::update_operation_many_previous(
    $crate::test_utils::fixtures::operation(
        Some($crate::test_utils::fixtures::operation_fields($crate::test_utils::constants::test_fields())),
        Some(DocumentViewId::new(&[
            $crate::test_utils::fixtures::random_operation_id(),
            $crate::test_utils::fixtures::random_operation_id(),
            $crate::test_utils::fixtures::random_operation_id()
            ]).unwrap()),
        None
        )

)]
#[allow(unused_qualifications)]
#[case::delete_operation_many_previous($crate::test_utils::fixtures::operation(
    None,
    Some(DocumentViewId::new(&[
        $crate::test_utils::fixtures::random_operation_id(),
        $crate::test_utils::fixtures::random_operation_id(),
        $crate::test_utils::fixtures::random_operation_id()
        ]).unwrap()),
        None
    )
)]
fn many_valid_operations(#[case] operation: Operation) {}

/// This template contains various types of valid meta-operation.
#[template]
#[export]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_meta_operation($crate::test_utils::fixtures::verified_operation(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    None,
    None,
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::update_meta_operation($crate::test_utils::fixtures::verified_operation(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    None,
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::delete_meta_operation($crate::test_utils::fixtures::verified_operation(
    None,
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    None,
    None,
    None
))]
fn many_verified_operations(#[case] operation: VerifiedOperation) {}

/// This template contains examples of all structs which implement the `AsOperation` trait.
#[template]
#[export]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_operation($crate::test_utils::fixtures::operation(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::update_operation($crate::test_utils::fixtures::operation(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    None
))]
#[allow(unused_qualifications)]
#[case::delete_operation($crate::test_utils::fixtures::operation(None, Some(
    $crate::test_utils::constants::HASH.parse().unwrap()),
    None
))]
#[allow(unused_qualifications)]
#[case::create_meta_operation($crate::test_utils::fixtures::verified_operation(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    None,
    None,
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::update_meta_operation($crate::test_utils::fixtures::verified_operation(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    None,
    None,
    None
))]
#[allow(unused_qualifications)]
#[case::delete_meta_operation($crate::test_utils::fixtures::verified_operation(
    None,
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    None,
    None,
    None
))]
fn implements_as_operation(#[case] operation: impl AsOperation) {}

pub use implements_as_operation;
pub use many_valid_operations;
pub use many_verified_operations;
