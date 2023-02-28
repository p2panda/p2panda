// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// This template contains various types of valid entries.
#[template]
#[export]
#[rstest]
#[allow(unused_qualifications)]
#[case::first_entry($crate::test_utils::fixtures::entry(
    1,
    1,
    None,
    None,
    $crate::test_utils::fixtures::encoded_create_operation_with_schema(),
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
#[allow(unused_qualifications)]
#[case::entry_with_backlink($crate::test_utils::fixtures::entry(
    2,
    1,
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    None,
    $crate::test_utils::fixtures::encoded_create_operation_with_schema(),
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
#[allow(unused_qualifications)]
#[case::entry_with_backlink_and_skiplink($crate::test_utils::fixtures::entry(
    13,
    1,
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    Some($crate::test_utils::fixtures::random_hash()),
    $crate::test_utils::fixtures::encoded_create_operation_with_schema(),
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
#[allow(unused_qualifications)]
#[case::skiplink_can_be_omitted_when_sam_as_backlink($crate::test_utils::fixtures::entry(
    14,
    1,
    Some($crate::test_utils::constants::HASH.parse().unwrap()),
    None,
    $crate::test_utils::fixtures::encoded_create_operation_with_schema(),
    $crate::test_utils::fixtures::key_pair(
        $crate::test_utils::constants::PRIVATE_KEY
    )
))]
pub fn many_valid_entries(#[case] entry: Entry) {}
