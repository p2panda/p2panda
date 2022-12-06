// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// This template contains various types of valid operation.
#[template]
#[export]
#[rstest]
#[allow(unused_qualifications)]
#[case($crate::test_utils::fixtures::operation_with_schema(
    Some(
        $crate::test_utils::fixtures::operation_fields(
            $crate::test_utils::constants::test_fields()
        )
    ),
    None,
))]
#[allow(unused_qualifications)]
#[case::update_operation(
    $crate::test_utils::fixtures::operation_with_schema(
        Some(
            $crate::test_utils::fixtures::operation_fields(
                $crate::test_utils::constants::test_fields()
            )
        ),
        Some($crate::test_utils::constants::HASH.parse().unwrap()),
    )
)]
#[allow(unused_qualifications)]
#[case::delete_operation(
    $crate::test_utils::fixtures::operation_with_schema(
        None,
        Some($crate::test_utils::constants::HASH.parse().unwrap()),
    )
)]
#[allow(unused_qualifications)]
#[case::update_operation_many_previous(
    $crate::test_utils::fixtures::operation_with_schema(
        Some(
            $crate::test_utils::fixtures::operation_fields(
                $crate::test_utils::constants::test_fields()
            )
        ),
        Some(DocumentViewId::new(&[
            $crate::test_utils::fixtures::random_operation_id(),
            $crate::test_utils::fixtures::random_operation_id(),
            $crate::test_utils::fixtures::random_operation_id()
        ])),
    )
)]
#[allow(unused_qualifications)]
#[case::delete_operation_many_previous($crate::test_utils::fixtures::operation_with_schema(
    None,
    Some(
        DocumentViewId::new(&[
            $crate::test_utils::fixtures::random_operation_id(),
            $crate::test_utils::fixtures::random_operation_id(),
            $crate::test_utils::fixtures::random_operation_id()
        ])
    ),
))]
fn many_valid_operations(#[case] operation: Operation) {}

/// This template contains various types of valid verified operations with entries.
#[template]
#[export]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_operation($crate::test_utils::fixtures::published_operation_with_schema(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    None,
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
#[allow(unused_qualifications)]
#[case::update_operation($crate::test_utils::fixtures::published_operation_with_schema(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
#[allow(unused_qualifications)]
#[case::delete_operation($crate::test_utils::fixtures::published_operation_with_schema(
    None,
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
fn many_published_operations(#[case] operation: PublishedOperation) {}

/// This template contains examples of all structs which implement the `AsOperation` trait.
#[template]
#[export]
#[rstest]
#[allow(unused_qualifications)]
#[case::create_operation($crate::test_utils::fixtures::operation_with_schema(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    None,
))]
#[allow(unused_qualifications)]
#[case::update_operation($crate::test_utils::fixtures::operation_with_schema(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
))]
#[allow(unused_qualifications)]
#[case::delete_operation($crate::test_utils::fixtures::operation_with_schema(
    None,
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
))]
#[allow(unused_qualifications)]
#[case::create_operation($crate::test_utils::fixtures::published_operation_with_schema(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    None,
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
#[allow(unused_qualifications)]
#[case::update_operation($crate::test_utils::fixtures::published_operation_with_schema(
    Some($crate::test_utils::fixtures::operation_fields(test_fields())),
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
#[allow(unused_qualifications)]
#[case::delete_operation($crate::test_utils::fixtures::published_operation_with_schema(
    None,
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
fn implements_as_operation(#[case] operation: impl AsOperation) {}

pub use implements_as_operation;
pub use many_valid_operations;
pub use many_published_operations;
