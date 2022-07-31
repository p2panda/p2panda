// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// This template contains various types of valid entries.
#[template]
#[export]
#[rstest]
#[allow(unused_qualifications)]
#[case::first_entry($crate::next::test_utils::fixtures::entry(
    1,
    1,
    None,
    None,
    Some($crate::next::test_utils::fixtures::operation(
        Some($crate::next::test_utils::fixtures::operation_fields(
            $crate::next::test_utils::constants::test_fields()
        )),
        None,
        None
    ))
))]
#[allow(unused_qualifications)]
#[case::entry_with_backlink($crate::next::test_utils::fixtures::entry(
    2,
    1,
    Some($crate::next::test_utils::constants::HASH.parse().unwrap()),
    None,
    Some($crate::next::test_utils::fixtures::operation(
        Some(
            $crate::next::test_utils::fixtures::operation_fields(
                $crate::next::test_utils::constants::test_fields()
            )
        ),
        None,
        None
    ))
))]
#[allow(unused_qualifications)]
#[case::entry_with_backlink_and_skiplink($crate::next::test_utils::fixtures::entry(
    13,
    1,
    Some($crate::next::test_utils::constants::HASH.parse().unwrap()),
    Some($crate::next::test_utils::fixtures::random_hash()),
    Some($crate::next::test_utils::fixtures::operation(
        Some(
            $crate::next::test_utils::fixtures::operation_fields(
                $crate::next::test_utils::constants::test_fields()
            )
        ),
        None,
        None
    ))
))]
#[allow(unused_qualifications)]
#[case::skiplink_can_be_omitted_when_sam_as_backlink($crate::next::test_utils::fixtures::entry(
    14,
    1,
    Some($crate::next::test_utils::constants::HASH.parse().unwrap()),
    None,
    Some($crate::next::test_utils::fixtures::operation(
        Some(
            $crate::next::test_utils::fixtures::operation_fields(
                $crate::next::test_utils::constants::test_fields()
            )
        ),
        None,
        None
    ))
))]
fn many_valid_entries(#[case] entry: Entry) {}

pub use many_valid_entries;
