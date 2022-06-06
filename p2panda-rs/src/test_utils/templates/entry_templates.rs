// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest_reuse::template;

/// This template contains various types of valid entries.
#[template]
#[rstest]
#[allow(unused_qualifications)]
#[case::first_entry(crate::test_utils::fixtures::entry(
    1,
    1,
    None,
    None,
    Some(crate::test_utils::fixtures::operation(
        Some(crate::test_utils::fixtures::operation_fields(crate::test_utils::constants::default_fields())), 
        None, 
        None
    ))
))]
#[allow(unused_qualifications)]
#[case::entry_with_backlink(crate::test_utils::fixtures::entry(
    2,
    1,
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    None,
    Some(crate::test_utils::fixtures::operation(
        Some(crate::test_utils::fixtures::operation_fields(crate::test_utils::constants::default_fields())), 
        None, 
        None
    ))
))]
#[allow(unused_qualifications)]
#[case::entry_with_backlink_and_skiplink(crate::test_utils::fixtures::entry(
    13,
    1,
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    Some(crate::test_utils::fixtures::operation(
        Some(crate::test_utils::fixtures::operation_fields(crate::test_utils::constants::default_fields())), 
        None, 
        None
    ))
))]
#[case::skiplink_can_be_omitted_when_sam_as_backlink(crate::test_utils::fixtures::entry(
    14,
    1,
    Some(crate::test_utils::constants::DEFAULT_HASH.parse().unwrap()),
    None,
    Some(crate::test_utils::fixtures::operation(
        Some(crate::test_utils::fixtures::operation_fields(crate::test_utils::constants::default_fields())), 
        None, 
        None
    ))
))]
fn many_valid_entries(#[case] entry: Entry) {}

#[allow(unused_imports)]
pub(crate) use many_valid_entries;
